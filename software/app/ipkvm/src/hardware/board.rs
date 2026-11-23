use super::{
    audio::Audio,
    hid::Hid,
    video::{hdmi_device, Video},
    Hardware,
};
use serde_json::{json, Value};
use std::{
    env,
    ffi::OsString,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
};
pub(super) fn device(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.into())
}
pub(super) fn run(command: &mut Command) -> io::Result<Vec<u8>> {
    let out = command.output()?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(io::Error::other(format!(
            "{}: {}",
            command.get_program().to_string_lossy(),
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}
fn named(dir: &Path, namefile: &str, name: &str) -> io::Result<PathBuf> {
    fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .find(|p| fs::read_to_string(p.join(namefile)).is_ok_and(|n| n.trim() == name))
        .ok_or(io::ErrorKind::NotFound.into())
}
fn number(p: &Path) -> io::Result<f64> {
    fs::read_to_string(p)?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or(io::ErrorKind::InvalidData.into())
}
fn illuminance(p: &Path) -> io::Result<f64> {
    match number(&p.join("in_illuminance_input")) {
        Ok(v) => Ok(v),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            Ok(number(&p.join("in_illuminance_raw"))? * number(&p.join("in_illuminance_scale"))?)
        }
        Err(e) => Err(e),
    }
}
fn backlight(p: &Path, percent: u64) -> io::Result<()> {
    let max = number(&p.join("max_brightness"))?;
    fs::write(
        p.join("brightness"),
        format!(
            "{}\n",
            (max * percent.min(100) as f64 / 100.).round() as u64
        ),
    )
}
fn usb_args(usb: &Value) -> Vec<(&'static str, String)> {
    let mut args = Vec::new();
    for (field, key) in [
        ("keyboard", "KEYBOARD"),
        ("mouse_rel", "REL_MOUSE"),
        ("mouse_abs", "ABS_MOUSE"),
        ("mouse_merge", "MERGE_MICE"),
        ("telemetry", "TELEMETRY"),
        ("storage", "STORAGE"),
        ("acm", "ACM"),
        ("microphone", "MICROPHONE"),
    ] {
        args.push((key, if usb[field] == true { "1" } else { "0" }.into()));
    }
    args.push((
        "NETWORK",
        if usb["net"] == true {
            usb["net_mode"].as_str().unwrap_or("off")
        } else {
            "off"
        }
        .into(),
    ));
    args
}
fn gpio(field: &str) -> io::Result<(String, String, bool)> {
    let line = env::var(format!("KVMD_GPIO_{field}"))
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("unconfigured board GPIO line {field}"),
            )
        })?;
    Ok((
        device("KVMD_GPIO_CHIP", "gpiochip0"),
        line.to_string(),
        device(&format!("KVMD_GPIO_{field}_ACTIVE_LOW"), "0") == "1",
    ))
}
fn indicator(field: &str) -> io::Result<bool> {
    let (chip, line, invert) = gpio(field)?;
    let out = run(Command::new("gpioget").args([chip, line]))?;
    match out.trim_ascii() {
        b"1" => Ok(!invert),
        b"0" => Ok(invert),
        _ => Err(io::ErrorKind::InvalidData.into()),
    }
}

#[derive(Default)]
pub(super) struct Board {
    audio: Mutex<Audio>,
    hid: Mutex<Hid>,
    enumeration: Mutex<()>,
    video: Mutex<Video>,
    pulse: Mutex<()>,
}
impl Hardware for Board {
    fn microphone_status(&self) -> &'static str {
        self.audio.lock().unwrap().status()
    }

    fn usb_bound(&self) -> io::Result<bool> {
        match fs::read_to_string("/sys/kernel/config/usb_gadget/kvmpanel/UDC") {
            Ok(name) => Ok(!name.trim().is_empty()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }
    fn type_key(&self, modifiers: u8, usage: u8) -> io::Result<()> {
        self.hid.lock().unwrap().type_key(modifiers, usage)
    }
    fn jiggle(&self) -> io::Result<()> {
        self.hid.lock().unwrap().jiggle()
    }
    fn apply_usb(&self, usb: &Value, media: &[(OsString, OsString)]) -> io::Result<()> {
        let _enumeration = self.enumeration.lock().unwrap();
        let mut audio = self.audio.lock().unwrap();
        audio.stop();
        // Release old device file descriptions before configfs unbind. Input during
        // re-enumeration observes absent ports without waiting on the external script.
        *self.hid.lock().unwrap() = Hid::default();
        let script = "/usr/sbin/kvmpanel-gadget";
        run(Command::new(script).arg("stop"))?;
        if crate::usb_budget(usb)["in"] != 0 {
            run(Command::new(script)
                .arg("start")
                .envs(usb_args(usb))
                .envs(media.iter().map(|(k, v)| (k, v))))?;
            let fresh = Hid::enumerate()?;
            *self.hid.lock().unwrap() = fresh;
            if usb["microphone"] == true {
                audio.start();
            }
        }
        Ok(())
    }
    fn frame(&self, q: u8) -> io::Result<axum::body::Bytes> {
        self.video.lock().unwrap().frame(q).map(Into::into)
    }
    fn stop_video(&self) {
        self.video.lock().unwrap().stop()
    }
    fn hdmi(&self) -> io::Result<Value> {
        Ok(self.video.lock().unwrap().hdmi())
    }
    fn hid(&self, msg: &Value) -> io::Result<()> {
        self.hid.lock().unwrap().input(msg)
    }
    fn reset_hid(&self) -> io::Result<()> {
        let _enumeration = self.enumeration.lock().unwrap();
        let mut hid = self.hid.lock().unwrap();
        *hid = Hid::enumerate()?;
        hid.reset()
    }
    fn release_hid(&self) -> io::Result<()> {
        self.hid.lock().unwrap().release_human()
    }
    fn begin_mcp_input(&self) -> io::Result<()> {
        self.hid.lock().unwrap().begin_mcp_input()
    }
    fn finish_mcp_input(&self) -> io::Result<()> {
        self.hid.lock().unwrap().finish_mcp_input()
    }
    fn locks(&self) -> io::Result<Value> {
        Ok(self.hid.lock().unwrap().locks())
    }
    fn telemetry(&self) -> io::Result<Option<Vec<u8>>> {
        self.hid.lock().unwrap().telemetry()
    }
    fn edid(&self, data: &[u8]) -> io::Result<()> {
        let mut video = self.video.lock().unwrap();
        video.stop();
        let mut cmd = Command::new("v4l2-ctl")
            .args([
                "-d",
                &hdmi_device(),
                "--keep-edid-checksums",
                "--set-edid=pad=0,file=-,format=raw",
            ])
            .stdin(Stdio::piped())
            .spawn()?;
        let result = cmd.stdin.take().unwrap().write_all(data);
        let status = cmd.wait()?;
        result?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other("set EDID failed"))
        }
    }
    fn read_edid(&self) -> io::Result<Vec<u8>> {
        run(Command::new("v4l2-ctl").args(["-d", &hdmi_device(), "--get-edid=pad=0,format=raw"]))
    }
    fn press(&self, button: &str, ms: u64) -> io::Result<()> {
        let field = match button {
            "power" => "POWER",
            "reset" => "RESET",
            _ => return Err(io::ErrorKind::InvalidInput.into()),
        };
        let _guard = self.pulse.lock().unwrap();
        let (chip, line, low) = gpio(field)?;
        let active = format!("{line}={}", (!low) as u8);
        let idle = format!("{line}={}", low as u8);
        let pulse = run(Command::new("gpioset").args([
            "-m",
            "time",
            "-s",
            &(ms / 1000).to_string(),
            "-u",
            &((ms % 1000) * 1000).to_string(),
            &chip,
            &active,
        ]));
        let release = run(Command::new("gpioset").args([&chip, &idle]));
        pulse.and(release).map(|_| ())
    }
    fn press_cancellable(
        &self,
        button: &str,
        ms: u64,
        cancelled: &(dyn Fn() -> bool + Sync),
    ) -> io::Result<()> {
        let field = match button {
            "power" => "POWER",
            "reset" => "RESET",
            _ => return Err(io::ErrorKind::InvalidInput.into()),
        };
        let _guard = self.pulse.lock().unwrap();
        if cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let (chip, line, low) = gpio(field)?;
        let active = format!("{line}={}", (!low) as u8);
        let idle = format!("{line}={}", low as u8);
        let mut pulse = Command::new("gpioset")
            .args(["-m", "signal", &chip, &active])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(ms);
        let result = loop {
            if cancelled() {
                break Err(io::ErrorKind::Interrupted.into());
            }
            if std::time::Instant::now() >= deadline {
                break Ok(());
            }
            match pulse.try_wait() {
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
                Ok(Some(status)) => break Err(io::Error::other(format!("gpioset: {status}"))),
                Err(error) => break Err(error),
            }
        };
        let _ = pulse.kill();
        let _ = pulse.wait();
        let release = run(Command::new("gpioset").args([&chip, &idle]));
        result.and(release.map(|_| ()))
    }
    fn mouse_buttons(&self, buttons: u8, absolute: bool) -> io::Result<()> {
        self.hid.lock().unwrap().mouse_buttons(buttons, absolute)
    }
    fn host_write(&self, data: &[u8; 64]) -> io::Result<()> {
        self.hid.lock().unwrap().host_write(data)
    }
    fn power_buttons(&self) -> (bool, bool) {
        (gpio("POWER").is_ok(), gpio("RESET").is_ok())
    }
    fn power(&self) -> io::Result<Value> {
        Ok(json!({"on":indicator("POWER_LED")?}))
    }
    fn lux(&self) -> io::Result<f64> {
        illuminance(&named(Path::new("/sys/bus/iio/devices"), "name", "bh1750")?)
    }
    fn temperature(&self) -> io::Result<f64> {
        Ok(
            number(&named(Path::new("/sys/class/thermal"), "type", "cpu-thermal")?.join("temp"))?
                / 1000.,
        )
    }
    fn brightness(&self, percent: u64) -> io::Result<()> {
        let path = if let Some(p) = env::var_os("KVMD_BACKLIGHT") {
            PathBuf::from(p)
        } else {
            let mut dirs = fs::read_dir("/sys/class/backlight")?;
            let entry = dirs.next().ok_or(io::ErrorKind::NotFound)??;
            if dirs.next().is_some() {
                return Err(io::Error::other("select backlight with KVMD_BACKLIGHT"));
            }
            entry.path()
        };
        backlight(&path, percent)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sysfs_scaling() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("iio:device3");
        fs::create_dir(&p).unwrap();
        fs::write(p.join("name"), "bh1750\n").unwrap();
        fs::write(p.join("in_illuminance_raw"), "600").unwrap();
        fs::write(p.join("in_illuminance_scale"), "0.5").unwrap();
        assert_eq!(
            illuminance(&named(dir.path(), "name", "bh1750").unwrap()).unwrap(),
            300.0
        );
        fs::write(p.join("max_brightness"), "255").unwrap();
        backlight(&p, 70).unwrap();
        assert_eq!(fs::read_to_string(p.join("brightness")).unwrap(), "179\n");
    }
    #[test]
    fn gadget_env() {
        let usb = json!({"keyboard":true,"mouse_merge":true,"net":true,"net_mode":"rndis"});
        let args = usb_args(&usb);
        for (k, v) in [
            ("NETWORK", "rndis"),
            ("MERGE_MICE", "1"),
            ("STORAGE", "0"),
            ("KEYBOARD", "1"),
        ] {
            assert!(args.contains(&(k, v.to_owned())))
        }
    }
}
