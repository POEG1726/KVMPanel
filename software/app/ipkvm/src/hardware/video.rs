use super::board::{device, run};
use serde_json::{json, Value};
use std::{
    io::{self, Read},
    os::fd::AsRawFd,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq)]
struct Mode {
    width: usize,
    height: usize,
    fps: f64,
}
fn timings(s: &str) -> Option<Mode> {
    let field = |name: &str| {
        s.lines()
            .find_map(|l| l.trim().strip_prefix(name))?
            .trim()
            .parse::<usize>()
            .ok()
    };
    let width = field("Active width:")?;
    let height = field("Active height:")?;
    let fps = s
        .lines()
        .find(|s| s.contains("per second)"))?
        .split_once('(')?
        .1
        .split_whitespace()
        .next()?
        .parse::<f64>()
        .ok()?;
    if !(32..=4088).contains(&width)
        || !(32..=4096).contains(&height)
        || width % 2 != 0
        || height % 2 != 0
        || !fps.is_finite()
    {
        return None;
    }
    Some(Mode { width, height, fps })
}

struct Capture {
    child: Child,
    mode: Mode,
}
impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Capture {
    fn start(mode: Mode) -> io::Result<Self> {
        let sub = device("KVMD_HDMI_DEVICE", "/dev/v4l-subdev0");
        run(Command::new("v4l2-ctl").args(["-d", &sub, "--set-dv-bt-timings", "query"]))?;
        let format = format!("{}x{}", mode.width, mode.height);
        run(Command::new("media-ctl").args([
            "-d",
            &device("KVMD_MEDIA_DEVICE", "/dev/media0"),
            "-V",
            &format!("\"sun20i-csi-bridge\":0 [fmt:UYVY8_2X8/{format} field:none]"),
        ]))?;
        let child = Command::new("v4l2-ctl")
            .args([
                "-d",
                &device("KVMD_VIDEO_DEVICE", "/dev/video0"),
                &format!(
                    "--set-fmt-video=width={},height={},pixelformat=YU12",
                    mode.width, mode.height
                ),
                "--stream-mmap=4",
                "--stream-to=-",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(Self { child, mode })
    }
    fn read(&mut self) -> io::Result<Vec<u8>> {
        let mut buf = vec![0; self.mode.width.next_multiple_of(8) * self.mode.height * 3 / 2];
        let out = self.child.stdout.as_mut().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut filled = 0;
        while filled < buf.len() {
            let mut p = libc::pollfd {
                fd: out.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let ms = deadline
                .saturating_duration_since(Instant::now())
                .as_millis() as i32;
            // The pipe stays owned by Child throughout this poll and read.
            let ready = unsafe { libc::poll(&mut p, 1, ms) };
            if ready < 0 {
                return Err(io::Error::last_os_error());
            }
            if ready == 0 || ms == 0 {
                return Err(io::ErrorKind::TimedOut.into());
            }
            let n = out.read(&mut buf[filled..])?;
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            filled += n;
        }
        Ok(buf)
    }
}

#[derive(Default)]
pub(super) struct Video {
    capture: Option<Capture>,
    mode: Option<(Instant, Option<Mode>)>,
}
impl Video {
    pub fn stop(&mut self) {
        self.capture = None;
    }
    fn mode(&mut self) -> Option<Mode> {
        if let Some((at, m)) = self.mode {
            if at.elapsed() < Duration::from_millis(500) {
                return m;
            }
        }
        let mode = run(Command::new("v4l2-ctl").args([
            "-d",
            &device("KVMD_HDMI_DEVICE", "/dev/v4l-subdev0"),
            "--query-dv-timings",
        ]))
        .ok()
        .and_then(|s| timings(&String::from_utf8_lossy(&s)));
        self.mode = Some((Instant::now(), mode));
        if self.capture.as_ref().is_some_and(|c| Some(c.mode) != mode) {
            self.stop()
        }
        mode
    }
    pub fn hdmi(&mut self) -> Value {
        match self.mode() {
            Some(m) => {
                json!({"signal":true,"width":m.width,"height":m.height,"fps":m.fps.round() as u32})
            }
            None => json!({"signal":false,"width":0,"height":0,"fps":0}),
        }
    }
    pub fn frame(&mut self, quality: u8) -> io::Result<Vec<u8>> {
        let mode = self.mode().ok_or(io::ErrorKind::NotConnected)?;
        if self.capture.is_none() {
            self.capture = Some(Capture::start(mode)?)
        }
        let raw = match self.capture.as_mut().unwrap().read() {
            Ok(raw) => raw,
            Err(e) => {
                self.stop();
                self.mode = None;
                return Err(e);
            }
        };
        encode(&raw, mode.width, mode.height, quality)
    }
}

// The Bootlin sysroot supplies libturbojpeg.a. On a native Linux PC the same
// ABI is loaded at capture time so --sim has no system library dependency.
type Init = unsafe extern "C" fn() -> *mut libc::c_void;
type Compress = unsafe extern "C" fn(
    *mut libc::c_void,
    *const *const u8,
    i32,
    *const i32,
    i32,
    i32,
    *mut *mut u8,
    *mut libc::c_ulong,
    i32,
    i32,
) -> i32;
type Destroy = unsafe extern "C" fn(*mut libc::c_void) -> i32;
type Free = unsafe extern "C" fn(*mut u8);
#[cfg(target_arch = "arm")]
#[link(name = "turbojpeg", kind = "static")]
extern "C" {
    fn tjInitCompress() -> *mut libc::c_void;
    fn tjCompressFromYUVPlanes(
        h: *mut libc::c_void,
        planes: *const *const u8,
        w: i32,
        strides: *const i32,
        height: i32,
        sub: i32,
        out: *mut *mut u8,
        size: *mut libc::c_ulong,
        q: i32,
        flags: i32,
    ) -> i32;
    fn tjDestroy(h: *mut libc::c_void) -> i32;
    fn tjFree(buf: *mut u8);
}
fn encode(raw: &[u8], w: usize, h: usize, q: u8) -> io::Result<Vec<u8>> {
    let stride = w.next_multiple_of(8);
    if w == 0 || h == 0 || w % 2 != 0 || h % 2 != 0 || raw.len() != stride * h * 3 / 2 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    #[cfg(not(target_arch = "arm"))]
    let lib = unsafe { libloading::Library::new("libturbojpeg.so.0") }.map_err(io::Error::other)?;
    // sun20i-csi aligns capture widths to eight pixels. Keep padding out
    // of the JPEG even for HDMI modes such as 1366 x 768.
    let luma = stride * h;
    let planes = [
        raw.as_ptr(),
        raw[luma..].as_ptr(),
        raw[luma + luma / 4..].as_ptr(),
    ];
    let strides = [stride as i32, (stride / 2) as i32, (stride / 2) as i32];
    unsafe {
        #[cfg(not(target_arch = "arm"))]
        let (init, compress, destroy, free): (Init, Compress, Destroy, Free) = (
            *lib.get(b"tjInitCompress\0").map_err(io::Error::other)?,
            *lib.get(b"tjCompressFromYUVPlanes\0")
                .map_err(io::Error::other)?,
            *lib.get(b"tjDestroy\0").map_err(io::Error::other)?,
            *lib.get(b"tjFree\0").map_err(io::Error::other)?,
        );
        #[cfg(target_arch = "arm")]
        let (init, compress, destroy, free): (Init, Compress, Destroy, Free) =
            (tjInitCompress, tjCompressFromYUVPlanes, tjDestroy, tjFree);
        let handle = init();
        if handle.is_null() {
            return Err(io::Error::other("tjInitCompress failed"));
        }
        let mut out = std::ptr::null_mut();
        let mut len = 0;
        let status = compress(
            handle,
            planes.as_ptr(),
            w as i32,
            strides.as_ptr(),
            h as i32,
            2,
            &mut out,
            &mut len,
            q.into(),
            2048,
        );
        let bytes = if status == 0 && !out.is_null() {
            Ok(std::slice::from_raw_parts(out, len as usize).to_vec())
        } else {
            Err(io::Error::other("JPEG compression failed"))
        };
        free(out);
        destroy(handle);
        bytes
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(target_arch = "arm")]
    fn arm_turbojpeg_encodes_yuv420() {
        let img = encode(&[128; 24 * 16 * 3 / 2], 22, 16, 80).unwrap();
        assert_eq!(&img[..2], &[0xff, 0xd8]);
        assert_eq!(&img[img.len() - 2..], &[0xff, 0xd9]);
    }
    #[test]
    fn capture_timings() {
        let sample="\tActive width: 1920\n\tActive height: 1080\n\tPixelclock: 74250000 Hz (30.00 frames per second)";
        assert_eq!(
            timings(sample),
            Some(Mode {
                width: 1920,
                height: 1080,
                fps: 30.
            })
        );
        assert!(timings("no signal").is_none());
        assert!(encode(&[0; 4], 640, 480, 80).is_err());
    }
}
