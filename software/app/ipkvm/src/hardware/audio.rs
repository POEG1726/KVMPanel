//! Bridge stereo DMIC capture through HiFi4 to USB Audio gadget playback.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::AsRawFd,
        unix::{fs::OpenOptionsExt, process::CommandExt},
    },
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};

const STATE: &str = "/run/kvmpanel-microphone";
const TAG: &[u8; 4] = b"KVA1";
const REPLY_TAG: &[u8; 4] = b"KVO1";
const FRAMES: usize = 60;
const INPUT: usize = FRAMES * 8;
const OUTPUT: usize = FRAMES * 3;

#[derive(Default)]
pub(super) struct Audio(Option<Child>);

impl Audio {
    pub(super) fn start(&mut self) {
        match std::env::current_exe().and_then(|exe| {
            Command::new(exe)
                .arg("--usb-audio-worker")
                .arg(std::process::id().to_string())
                .process_group(0)
                .spawn()
        }) {
            Ok(child) => self.0 = Some(child),
            Err(err) => eprintln!("kvmd: start USB microphone: {err}"),
        }
    }

    pub(super) fn stop(&mut self) {
        if let Some(mut worker) = self.0.take() {
            // Stop the PCM children before removing the gadget ALSA card.
            if matches!(worker.try_wait(), Ok(None)) {
                unsafe { libc::kill(-(worker.id() as i32), libc::SIGKILL) };
                let _ = worker.wait();
            }
        }
        let _ = fs::remove_file(STATE);
    }

    pub(super) fn status(&mut self) -> &'static str {
        match self.0.as_mut().map(Child::try_wait) {
            Some(Ok(None)) => match fs::read_to_string(STATE).as_deref() {
                Ok("ready") => "ready",
                Ok("unavailable") => "unavailable",
                _ => "starting",
            },
            _ => "unavailable",
        }
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        if self.0.is_some() {
            self.stop();
        }
    }
}

fn spawn_owned(command: &mut Command) -> io::Result<Child> {
    let parent = std::process::id();
    // The bridge starts both PCM children from its long-lived main thread.
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0) != 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::getppid() as u32 != parent {
                return Err(io::Error::from_raw_os_error(libc::ESRCH));
            }
            Ok(())
        })
    }
    .spawn()
}

struct Pcm(Child);

impl Drop for Pcm {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn state(value: &str) {
    let temp = "/run/.kvmpanel-microphone-next";
    let _ = fs::write(temp, value).and_then(|()| fs::rename(temp, STATE));
}

fn named_entry(dir: &str, field: &str, value: &str) -> io::Result<std::path::PathBuf> {
    fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .find(|p| fs::read_to_string(p.join(field)).is_ok_and(|v| v.trim() == value))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{value} in {dir}")))
}

fn open_dsp() -> io::Result<File> {
    let rproc = named_entry("/sys/class/remoteproc", "name", "1700000.remoteproc")?;
    let state = rproc.join("state");
    if fs::read_to_string(&state)?.trim() == "offline" {
        fs::write(state, "start")?;
    }
    // HiFi4 uses RPMsg destination 2. The C906 echo endpoint uses 1.
    for _ in 0..50 {
        if let Ok(endpoint) = named_entry("/sys/class/rpmsg", "dst", "2") {
            let name = endpoint.file_name().unwrap();
            match OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(Path::new("/dev").join(name))
            {
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                result => return result,
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "HiFi4 RPMsg endpoint",
    ))
}

fn wait_pcm(fd: std::os::fd::RawFd, timeout_ms: i32, source: &'static str) -> io::Result<()> {
    let mut poll = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    match unsafe { libc::poll(&mut poll, 1, timeout_ms) } {
        0 => Err(io::Error::new(io::ErrorKind::TimedOut, source)),
        -1 => Err(io::Error::last_os_error()),
        _ => Ok(()),
    }
}

fn exchange(dsp: &mut File, input: &[u8], reply: &mut [u8]) -> io::Result<()> {
    if dsp.write(input)? != input.len() {
        return Err(io::Error::new(
            io::ErrorKind::WriteZero,
            "HiFi4 RPMsg packet",
        ));
    }
    wait_pcm(dsp.as_raw_fd(), 100, "HiFi4 audio reply")?;
    let len = dsp.read(reply)?;
    let wanted = if input.len() == TAG.len() {
        TAG.len()
    } else {
        TAG.len() + OUTPUT
    };
    if len != wanted || &reply[..TAG.len()] != REPLY_TAG {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "HiFi4 audio protocol",
        ));
    }
    Ok(())
}

fn pcm(program: &str, device: &str, channels: &str, format: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.args([
        "-q",
        "-t",
        "raw",
        "-D",
        device,
        "-r",
        "48000",
        "-f",
        format,
        "-c",
        channels,
        "--period-size=240",
        "--buffer-size=1920",
    ]);
    cmd
}

fn stream() -> io::Result<()> {
    let cards = fs::read_to_string("/proc/asound/cards")?;
    let card = cards
        .lines()
        .filter(|line| line.contains("UAC2_Gadget"))
        .filter_map(|line| line.split_whitespace().next())
        .find_map(|number| number.parse::<u32>().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "UAC2 gadget ALSA card"))?;
    let mut dsp = open_dsp()?;
    let mut reply = [0; 496];
    exchange(&mut dsp, TAG, &mut reply)?;

    let mut playback = Pcm(spawn_owned(
        pcm("aplay", &format!("hw:{card},0"), "1", "S24_3LE").stdin(Stdio::piped()),
    )?);
    let mut output = playback.0.stdin.take().unwrap();
    // USB and DMIC have independent clocks, and the host can close its stream.
    // Bound the queued audio and keep reading DMIC when USB backs up.
    let fd = output.as_raw_fd();
    if unsafe { libc::fcntl(fd, libc::F_SETPIPE_SZ, 4096) } < 0
        || unsafe { libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK) } < 0
    {
        return Err(io::Error::last_os_error());
    }
    let mut capture = Pcm(spawn_owned(
        pcm("arecord", "hw:CARD=KVMPanelDMIC,DEV=0", "2", "S24_LE").stdout(Stdio::piped()),
    )?);
    let mut input = capture.0.stdout.take().unwrap();
    let mut packet = [0; 4 + INPUT];
    packet[..4].copy_from_slice(TAG);
    let mut ready = false;
    loop {
        wait_pcm(input.as_raw_fd(), 1000, "T113 DMIC capture")?;
        input.read_exact(&mut packet[4..])?;
        exchange(&mut dsp, &packet, &mut reply)?;
        match output.write_all(&reply[4..4 + OUTPUT]) {
            Ok(()) if !ready => {
                state("ready");
                ready = true;
            }
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
            Err(err) => return Err(err),
        }
    }
}

pub(super) fn worker(parent: u32) -> io::Result<()> {
    let owner = || unsafe { libc::getppid() as u32 };
    if owner() != parent || parent <= 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "USB audio worker parent",
        ));
    }
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(1));
        if owner() != parent {
            std::process::exit(0);
        }
    });
    let mut previous = String::new();
    loop {
        let err = stream().unwrap_err().to_string();
        state("unavailable");
        if err != previous {
            eprintln!("kvmd: USB microphone: {err}");
            previous = err;
        }
        thread::sleep(Duration::from_secs(2));
    }
}
