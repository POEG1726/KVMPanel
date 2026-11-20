use axum::body::Bytes;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{
    ffi::OsString,
    io,
    sync::{Arc, Mutex},
};

mod board;
mod hid;
pub(crate) mod layout;
mod pattern;
mod video;
pub trait Hardware: Send + Sync {
    fn stop_video(&self) {}
    fn usb_bound(&self) -> io::Result<bool>;
    fn apply_usb(&self, usb: &Value, media: &[(OsString, OsString)]) -> io::Result<()>;
    fn frame(&self, quality: u8) -> io::Result<Bytes>;
    fn type_key(&self, modifiers: u8, usage: u8) -> io::Result<()>;
    fn jiggle(&self) -> io::Result<()>;
    fn hid(&self, msg: &Value) -> io::Result<()>;
    fn press(&self, button: &str, duration_ms: u64) -> io::Result<()>;
    fn power(&self) -> io::Result<Value>;
    fn hdmi(&self) -> io::Result<Value>;
    fn locks(&self) -> io::Result<Value>;
    fn reset_hid(&self) -> io::Result<()>;
    fn release_hid(&self) -> io::Result<()> {
        self.reset_hid()
    }
    fn read_edid(&self) -> io::Result<Vec<u8>> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn edid(&self, data: &[u8]) -> io::Result<()>;
    fn telemetry(&self) -> io::Result<Option<Vec<u8>>>;
    fn lux(&self) -> io::Result<f64>;
    fn temperature(&self) -> io::Result<f64>;
    fn brightness(&self, percent: u64) -> io::Result<()>;
}

struct Sim {
    #[cfg(test)]
    gadget: Option<std::path::PathBuf>,
    frame_index: AtomicUsize,
    power: Mutex<bool>,
    locks: Mutex<Value>,
}
impl Hardware for Sim {
    fn usb_bound(&self) -> io::Result<bool> {
        Ok(true)
    }
    fn type_key(&self, _: u8, _: u8) -> io::Result<()> {
        Ok(())
    }
    fn jiggle(&self) -> io::Result<()> {
        Ok(())
    }
    fn apply_usb(&self, _: &Value, _: &[(OsString, OsString)]) -> io::Result<()> {
        #[cfg(test)]
        if let Some(path) = &self.gadget {
            if !std::process::Command::new("sh")
                .arg(path)
                .status()?
                .success()
            {
                return Err(io::Error::other("fake gadget failed"));
            }
        }
        Ok(())
    }
    fn frame(&self, quality: u8) -> io::Result<Bytes> {
        pattern::pattern(self.frame_index.fetch_add(1, Ordering::Relaxed), quality)
    }
    fn hid(&self, msg: &Value) -> io::Result<()> {
        if msg["t"] == "key" && msg["down"] == true {
            let key = match msg["code"].as_str() {
                Some("CapsLock") => "caps",
                Some("NumLock") => "num",
                Some("ScrollLock") => "scroll",
                _ => return Ok(()),
            };
            let mut locks = self.locks.lock().unwrap();
            locks[key] = json!(locks[key] != true);
        }
        Ok(())
    }
    fn press(&self, button: &str, ms: u64) -> io::Result<()> {
        if button == "power" {
            let mut on = self.power.lock().unwrap();
            *on = ms < 4000 && !*on
        }
        Ok(())
    }
    fn power(&self) -> io::Result<Value> {
        Ok(json!({"on":*self.power.lock().unwrap(),"hdd":false}))
    }
    fn hdmi(&self) -> io::Result<Value> {
        Ok(json!({"signal":true,"width":1920,"height":1080,"fps":60}))
    }
    fn locks(&self) -> io::Result<Value> {
        Ok(self.locks.lock().unwrap().clone())
    }
    fn reset_hid(&self) -> io::Result<()> {
        Ok(())
    }
    fn edid(&self, _: &[u8]) -> io::Result<()> {
        Ok(())
    }
    fn telemetry(&self) -> io::Result<Option<Vec<u8>>> {
        Ok(None)
    }
    fn lux(&self) -> io::Result<f64> {
        Ok(212.)
    }
    fn temperature(&self) -> io::Result<f64> {
        Ok(48.)
    }
    fn brightness(&self, _: u64) -> io::Result<()> {
        Ok(())
    }
}
pub fn open(sim: bool) -> io::Result<Arc<dyn Hardware>> {
    if !sim {
        return Ok(Arc::new(board::Board::default()));
    }
    Ok(Arc::new(Sim::new()))
}
impl Sim {
    fn new() -> Self {
        Self {
            #[cfg(test)]
            gadget: None,
            frame_index: AtomicUsize::new(0),
            power: Mutex::new(true),
            locks: Mutex::new(json!({"num":false,"caps":false,"scroll":false})),
        }
    }
}
#[cfg(test)]
pub(crate) fn simulated_gadget(path: std::path::PathBuf) -> Arc<dyn Hardware> {
    Arc::new(Sim {
        gadget: Some(path),
        ..Sim::new()
    })
}
