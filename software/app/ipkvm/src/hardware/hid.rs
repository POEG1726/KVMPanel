use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    thread,
    time::{Duration, Instant},
};

const FUNCTIONS: &str = "/sys/kernel/config/usb_gadget/kvmpanel/functions";
fn send(mut f: &File, report: &[u8]) -> io::Result<()> {
    let start = Instant::now();
    loop {
        match f.write(report) {
            Ok(n) if n == report.len() => return Ok(()),
            Ok(_) => return Err(io::ErrorKind::WriteZero.into()),
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock
                    && start.elapsed() < Duration::from_millis(250) =>
            {
                thread::sleep(Duration::from_millis(2))
            }
            Err(e) => return Err(e),
        }
    }
}
fn read(mut f: &File) -> io::Result<Option<Vec<u8>>> {
    let mut buf = [0u8; 64];
    match f.read(&mut buf) {
        Ok(0) => Ok(None),
        Ok(n) => Ok(Some(buf[..n].to_vec())),
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
        Err(e) => Err(e),
    }
}
pub(crate) fn keycode(code: &str) -> Option<u8> {
    let bytes = code.as_bytes();
    if let [b'K', b'e', b'y', c @ b'A'..=b'Z'] = bytes {
        return Some(4 + c - b'A');
    }
    if let [b'D', b'i', b'g', b'i', b't', d @ b'0'..=b'9'] = bytes {
        return Some(if *d == b'0' { 39 } else { 30 + d - b'1' });
    }
    if let Some(n) = code.strip_prefix('F').and_then(|v| v.parse::<u8>().ok()) {
        if (1..=12).contains(&n) {
            return Some(57 + n);
        }
        if (13..=24).contains(&n) {
            return Some(104 + n - 13);
        }
    }
    Some(match code {
        "ControlLeft" => 224,
        "ShiftLeft" => 225,
        "AltLeft" => 226,
        "MetaLeft" | "OSLeft" => 227,
        "ControlRight" => 228,
        "ShiftRight" => 229,
        "AltRight" => 230,
        "MetaRight" | "OSRight" => 231,
        "Enter" => 40,
        "Escape" => 41,
        "Backspace" => 42,
        "Tab" => 43,
        "Space" => 44,
        "Minus" => 45,
        "Equal" => 46,
        "BracketLeft" => 47,
        "BracketRight" => 48,
        "Backslash" => 49,
        "IntlHash" => 50,
        "Semicolon" => 51,
        "Quote" => 52,
        "Backquote" => 53,
        "Comma" => 54,
        "Period" => 55,
        "Slash" => 56,
        "CapsLock" => 57,
        "PrintScreen" => 70,
        "ScrollLock" => 71,
        "Pause" => 72,
        "Insert" => 73,
        "Home" => 74,
        "PageUp" => 75,
        "Delete" => 76,
        "End" => 77,
        "PageDown" => 78,
        "ArrowRight" => 79,
        "ArrowLeft" => 80,
        "ArrowDown" => 81,
        "ArrowUp" => 82,
        "NumLock" => 83,
        "NumpadDivide" => 84,
        "NumpadMultiply" => 85,
        "NumpadSubtract" => 86,
        "NumpadAdd" => 87,
        "NumpadEnter" => 88,
        "Numpad1" => 89,
        "Numpad2" => 90,
        "Numpad3" => 91,
        "Numpad4" => 92,
        "Numpad5" => 93,
        "Numpad6" => 94,
        "Numpad7" => 95,
        "Numpad8" => 96,
        "Numpad9" => 97,
        "Numpad0" => 98,
        "NumpadDecimal" => 99,
        "IntlBackslash" => 100,
        "ContextMenu" => 101,
        "Power" => 102,
        "NumpadEqual" => 103,
        "IntlRo" => 135,
        "KanaMode" => 136,
        "IntlYen" => 137,
        "Convert" => 138,
        "NonConvert" => 139,
        _ => return None,
    })
}
#[derive(Default, Clone)]
struct Keyboard {
    modifiers: u8,
    keys: Vec<u8>,
}
impl Keyboard {
    fn change(&mut self, k: u8, down: bool) {
        if (224..=231).contains(&k) {
            let bit = 1 << (k - 224);
            if down {
                self.modifiers |= bit
            } else {
                self.modifiers &= !bit
            }
        } else if down {
            if !self.keys.contains(&k) {
                self.keys.push(k)
            }
        } else {
            self.keys.retain(|x| *x != k)
        }
    }
    fn report(&self) -> [u8; 8] {
        let mut b = [0; 8];
        b[0] = self.modifiers;
        if self.keys.len() > 6 {
            b[2..].fill(1)
        } else {
            b[2..2 + self.keys.len()].copy_from_slice(&self.keys)
        }
        b
    }
}

struct SuspendedInput {
    keyboard: Keyboard,
    buttons: u8,
    absolute: bool,
    relative_motion: bool,
}

#[derive(Default)]
pub(super) struct Hid {
    ports: HashMap<&'static str, File>,
    keyboard: Keyboard,
    buttons: u8,
    position: [u16; 2],
    last_abs: bool,
    led: u8,
    suspended: Option<SuspendedInput>,
}
impl Hid {
    pub fn enumerate() -> io::Result<Self> {
        Self::open(Path::new(FUNCTIONS), Path::new("/dev"))
    }
    fn open(functions: &Path, dev_root: &Path) -> io::Result<Self> {
        let mut hid = Self::default();
        for name in ["keyboard", "relative", "absolute", "merged", "telemetry"] {
            let dev = match fs::read_to_string(functions.join(format!("hid.{name}/dev"))) {
                Ok(dev) => dev,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            };
            let minor = dev
                .trim()
                .split_once(':')
                .and_then(|(_, m)| m.parse::<u32>().ok())
                .ok_or(io::ErrorKind::InvalidData)?;
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(dev_root.join(format!("hidg{minor}")))?;
            hid.ports.insert(name, file);
        }
        Ok(hid)
    }
    fn send(&self, name: &str, report: &[u8]) -> io::Result<()> {
        let file = self.ports.get(name).ok_or(io::ErrorKind::NotConnected)?;
        send(file, report)
    }
    fn key(&mut self, k: u8, down: bool) -> io::Result<()> {
        let mut next = self.keyboard.clone();
        next.change(k, down);
        self.send("keyboard", &next.report())?;
        self.keyboard = next;
        Ok(())
    }
    fn mouse(&mut self, absolute: bool, rel: [i8; 2], wheel: i8) -> io::Result<()> {
        let merged = self.ports.contains_key("merged");
        let name = if merged {
            "merged"
        } else if absolute {
            "absolute"
        } else {
            "relative"
        };
        let mut report = Vec::with_capacity(7);
        if absolute || merged {
            report.push(if absolute { 2 } else { 1 })
        }
        report.push(self.buttons);
        if absolute {
            for coord in self.position {
                report.extend_from_slice(&(coord >> 1).to_le_bytes())
            }
        } else {
            report.extend(rel.map(|v| v as u8))
        }
        report.push(wheel as u8);
        self.send(name, &report)
    }
    /// A text stroke may require modifiers different from the user's held keys.
    /// Restore the current keyboard report even if pressing or releasing fails.
    pub fn type_key(&self, modifiers: u8, usage: u8) -> io::Result<()> {
        type_stroke(self.keyboard.report(), modifiers, usage, |report| {
            self.send("keyboard", report)
        })
    }

    pub fn jiggle(&mut self) -> io::Result<()> {
        // Do not drag a selection, and do not jump an absolute pointer to an unknown starting position.
        if self.buttons != 0 || self.suspended.is_some() {
            return Ok(());
        }
        if self.ports.contains_key("relative") || self.ports.contains_key("merged") {
            self.mouse(false, [1, 0], 0)?;
            return self.mouse(false, [-1, 0], 0);
        }
        if self.last_abs && self.ports.contains_key("absolute") {
            let old = self.position;
            self.position[0] ^= 2; // Bit zero is discarded when converting to the gadget's 15-bit range.
            let moved = self.mouse(true, [0, 0], 0);
            self.position = old;
            let restored = self.mouse(true, [0, 0], 0);
            moved.and(restored)
        } else {
            Ok(())
        }
    }
    pub fn input(&mut self, m: &Value) -> io::Result<()> {
        let code = |v: &Value| {
            v.as_str()
                .and_then(keycode)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unsupported HID key"))
        };
        match m["t"].as_str() {
            Some("key") => self.key(code(&m["code"])?, m["down"] == true),
            Some("shortcut") => {
                let keys = m["keys"]
                    .as_array()
                    .ok_or(io::ErrorKind::InvalidInput)?
                    .iter()
                    .map(code)
                    .collect::<io::Result<Vec<_>>>()?;
                let old = self.keyboard.clone();
                let result = (|| {
                    for &k in &keys {
                        self.key(k, true)?
                    }
                    for &k in keys.iter().rev() {
                        self.key(k, false)?
                    }
                    Ok(())
                })();
                self.keyboard = old;
                let restored = self.send("keyboard", &self.keyboard.report());
                result.and(restored)
            }
            Some("mouse_abs") | Some("mouse_rel") => {
                let absolute = m["t"] == "mouse_abs";
                let buttons = m["buttons"].as_u64().unwrap_or(0) as u8;
                let motion = [
                    m["dx"].as_i64().unwrap_or(0) as i8,
                    m["dy"].as_i64().unwrap_or(0) as i8,
                ];
                self.update_mouse_mode(buttons, absolute)?;
                if absolute {
                    self.position = [
                        m["x"].as_u64().unwrap_or(0) as u16,
                        m["y"].as_u64().unwrap_or(0) as u16,
                    ];
                }
                self.mouse(absolute, motion, 0)?;
                if let Some(saved) = &mut self.suspended {
                    if absolute {
                        saved.relative_motion = false;
                    } else if motion != [0, 0] {
                        saved.relative_motion = true;
                    }
                }
                Ok(())
            }
            Some("wheel") => {
                let abs = if self.ports.contains_key("merged") {
                    self.last_abs
                } else {
                    !self.ports.contains_key("relative")
                };
                self.mouse(abs, [0, 0], m["dy"].as_i64().unwrap_or(0) as i8)
            }
            _ => Err(io::ErrorKind::InvalidInput.into()),
        }
    }
    pub fn reset(&mut self) -> io::Result<()> {
        if self.ports.keys().all(|name| *name == "telemetry") {
            return Err(io::Error::from_raw_os_error(libc::ENODEV));
        }
        self.keyboard = Keyboard::default();
        self.buttons = 0;
        self.suspended = None;
        if self.ports.contains_key("keyboard") {
            self.send("keyboard", &self.keyboard.report())?
        }
        for (abs, name) in [(false, "relative"), (true, "absolute")] {
            if self.ports.contains_key("merged") || self.ports.contains_key(name) {
                self.mouse(abs, [0, 0], 0)?
            }
        }
        Ok(())
    }

    pub fn release_human(&mut self) -> io::Result<()> {
        // A console disconnect invalidates the saved human state, even during MCP input.
        if let Some(saved) = &mut self.suspended {
            saved.keyboard = Keyboard::default();
            saved.buttons = 0;
            Ok(())
        } else {
            self.reset()
        }
    }

    /// Start with neutral HID state so a human modifier cannot affect a tool's chord.
    pub fn begin_mcp_input(&mut self) -> io::Result<()> {
        if self.suspended.is_some() {
            return Ok(());
        }
        let held = self.keyboard.report();
        if held != [0; 8] {
            self.send("keyboard", &[0; 8])?;
        }
        let buttons = std::mem::take(&mut self.buttons);
        if buttons != 0 {
            if let Err(error) = self.mouse(self.last_abs, [0, 0], 0) {
                self.buttons = buttons;
                if held != [0; 8] {
                    let _ = self.send("keyboard", &held);
                }
                return Err(error);
            }
        }
        self.suspended = Some(SuspendedInput {
            keyboard: std::mem::take(&mut self.keyboard),
            buttons,
            absolute: self.last_abs,
            relative_motion: false,
        });
        Ok(())
    }

    /// Release only the tool's state and restore any human input that is still held.
    pub fn finish_mcp_input(&mut self) -> io::Result<()> {
        let Some(saved) = self.suspended.take() else {
            return Ok(());
        };
        let keys = if self.keyboard.report() != [0; 8] {
            self.send("keyboard", &[0; 8])
        } else {
            Ok(())
        };
        self.keyboard = saved.keyboard;
        let restored_keys = self.keyboard.report();
        let keyboard = if restored_keys != [0; 8] {
            keys.and(self.send("keyboard", &restored_keys))
        } else {
            keys
        };

        let buttons = if std::mem::take(&mut self.buttons) != 0 {
            self.mouse(self.last_abs, [0, 0], 0)
        } else {
            Ok(())
        };
        // An absolute report after relative travel would snap to a stale position.
        // A subsequent human pointer event supplies a fresh position and mask.
        let mouse = if saved.buttons != 0 && !(saved.absolute && saved.relative_motion) {
            self.buttons = saved.buttons;
            self.last_abs = saved.absolute;
            buttons.and(self.mouse(saved.absolute, [0, 0], 0))
        } else {
            buttons
        };
        keyboard.and(mouse)
    }
    pub fn locks(&mut self) -> Value {
        if let Some(b) = self
            .ports
            .get("keyboard")
            .and_then(|f| read(f).ok().flatten())
        {
            self.led = b[0]
        }
        json!({"num":self.led&1!=0,"caps":self.led&2!=0,"scroll":self.led&4!=0})
    }
    fn update_mouse_mode(&mut self, buttons: u8, absolute: bool) -> io::Result<()> {
        if self.suspended.is_some() && self.last_abs != absolute && self.buttons != 0 {
            let held = std::mem::take(&mut self.buttons);
            if let Err(error) = self.mouse(self.last_abs, [0, 0], 0) {
                self.buttons = held;
                return Err(error);
            }
        }
        self.buttons = buttons;
        self.last_abs = absolute;
        Ok(())
    }
    pub fn mouse_buttons(&mut self, buttons: u8, absolute: bool) -> io::Result<()> {
        self.update_mouse_mode(buttons, absolute)?;
        self.mouse(absolute, [0, 0], 0)
    }
    pub fn host_write(&self, packet: &[u8; 64]) -> io::Result<()> {
        let name = ["merged", "absolute", "telemetry"]
            .into_iter()
            .find(|name| self.ports.contains_key(name))
            .ok_or(io::ErrorKind::NotConnected)?;
        self.send(name, packet)
    }
    pub fn telemetry(&self) -> io::Result<Option<Vec<u8>>> {
        for name in ["merged", "absolute", "telemetry"] {
            if let Some(file) = self.ports.get(name) {
                return read(file);
            }
        }
        Ok(None)
    }
}
fn type_stroke(
    held: [u8; 8],
    modifiers: u8,
    usage: u8,
    mut send: impl FnMut(&[u8; 8]) -> io::Result<()>,
) -> io::Result<()> {
    if held != [0; 8] {
        send(&[0; 8])?;
    }
    let result = send(&[modifiers, 0, usage, 0, 0, 0, 0, 0]).and_then(|()| send(&[0; 8]));
    if held != [0; 8] {
        let restored = send(&held);
        return result.and(restored);
    }
    if result.is_err() {
        let _ = send(&[0; 8]);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_releases_its_own_input_and_does_not_restore_stale_pointer_coordinates() {
        let dir = tempfile::tempdir().unwrap();
        let mut hid = Hid::default();
        for name in ["keyboard", "relative", "absolute"] {
            hid.ports
                .insert(name, File::create(dir.path().join(name)).unwrap());
        }
        hid.input(&json!({"t":"key","code":"ControlLeft","down":true}))
            .unwrap();
        hid.input(&json!({"t":"mouse_abs","x":400,"y":800,"buttons":1}))
            .unwrap();
        hid.begin_mcp_input().unwrap();
        hid.input(&json!({"t":"key","code":"KeyB","down":true}))
            .unwrap();
        hid.input(&json!({"t":"mouse_rel","dx":10,"dy":0,"buttons":2}))
            .unwrap();
        hid.finish_mcp_input().unwrap();

        let key_reports = fs::read(dir.path().join("keyboard")).unwrap();
        let reports: Vec<_> = key_reports.chunks_exact(8).collect();
        assert_eq!(
            reports,
            [
                [1, 0, 0, 0, 0, 0, 0, 0],
                [0; 8],
                [0, 0, 5, 0, 0, 0, 0, 0],
                [0; 8],
                [1, 0, 0, 0, 0, 0, 0, 0],
            ]
        );
        assert_eq!(fs::read(dir.path().join("absolute")).unwrap().len(), 14);
        assert_eq!(
            fs::read(dir.path().join("relative")).unwrap(),
            [2, 10, 0, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn console_disconnect_discards_saved_keyboard_without_interrupting_mcp() {
        let dir = tempfile::tempdir().unwrap();
        let mut hid = Hid::default();
        hid.ports.insert(
            "keyboard",
            File::create(dir.path().join("keyboard")).unwrap(),
        );
        hid.key(224, true).unwrap();
        hid.begin_mcp_input().unwrap();
        hid.key(5, true).unwrap();
        hid.release_human().unwrap();
        assert_eq!(hid.keyboard.report()[2], 5);
        hid.finish_mcp_input().unwrap();
        assert_eq!(hid.keyboard.report(), [0; 8]);
    }
    #[test]
    fn reports_use_open_files_until_the_next_enumeration() {
        let dir = tempfile::tempdir().unwrap();
        let functions = dir.path().join("functions");
        let function = functions.join("hid.relative");
        fs::create_dir_all(&function).unwrap();
        let devno = function.join("dev");
        fs::write(&devno, "243:4").unwrap();
        for n in ["hidg4", "hidg7"] {
            fs::write(dir.path().join(n), []).unwrap();
        }
        let mut hid = Hid::open(&functions, dir.path()).unwrap();
        // configfs and the device path may change; a bound report still uses its open fd.
        fs::write(&devno, "243:7").unwrap();
        fs::rename(dir.path().join("hidg4"), dir.path().join("old")).unwrap();
        let motion = json!({"t":"mouse_rel","dx":3,"dy":-2,"buttons":1});
        hid.input(&motion).unwrap();
        hid.input(&motion).unwrap();
        assert_eq!(
            fs::read(dir.path().join("old")).unwrap(),
            [1, 3, 254, 0, 1, 3, 254, 0]
        );
        hid = Hid::open(&functions, dir.path()).unwrap();
        hid.input(&motion).unwrap();
        assert_eq!(fs::read(dir.path().join("hidg7")).unwrap(), [1, 3, 254, 0]);
    }
    #[test]
    fn keyboard_report_and_paste() {
        let mut keys = Keyboard::default();
        for code in ["ControlLeft", "AltLeft", "Delete"] {
            keys.change(keycode(code).unwrap(), true)
        }
        assert_eq!(keys.report(), [5, 0, 76, 0, 0, 0, 0, 0]);
        keys.change(224, false);
        assert_eq!(keys.report()[0], 4);
        let held = keys.report();
        let mut reports = Vec::new();
        type_stroke(held, 2, 29, |r| {
            reports.push(*r);
            Ok(())
        })
        .unwrap();
        assert_eq!(reports, [[0; 8], [2, 0, 29, 0, 0, 0, 0, 0], [0; 8], held]);
        reports.clear();
        let e = type_stroke(held, 2, 29, |r| {
            reports.push(*r);
            if reports.len() == 2 {
                Err(io::ErrorKind::BrokenPipe.into())
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(reports.last(), Some(&held));
    }
}
