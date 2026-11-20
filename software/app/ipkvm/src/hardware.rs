use serde_json::{json, Value};
use std::{
    ffi::OsString,
    io,
    sync::{Arc, Mutex},
};

pub trait Hardware: Send + Sync {
    fn stop_video(&self) {}
    fn apply_usb(&self, usb: &Value, media: &[(OsString, OsString)]) -> io::Result<()>;
    fn frame(&self, quality: u8) -> io::Result<Vec<u8>>;
    fn hid(&self, msg: &Value) -> io::Result<()>;
    fn press(&self, button: &str, duration_ms: u64) -> io::Result<()>;
    fn power(&self) -> io::Result<Value>;
    fn hdmi(&self) -> io::Result<Value>;
    fn locks(&self) -> io::Result<Value>;
    fn reset_hid(&self) -> io::Result<()>;
    fn edid(&self, data: &[u8]) -> io::Result<()>;
    fn telemetry(&self) -> io::Result<Option<Vec<u8>>>;
    fn lux(&self) -> io::Result<f64>;
    fn temperature(&self) -> io::Result<f64>;
    fn brightness(&self, percent: u64) -> io::Result<()>;
}

struct Sim {
    power: Mutex<bool>,
    locks: Mutex<Value>,
}
impl Hardware for Sim {
    fn apply_usb(&self, _: &Value, _: &[(OsString, OsString)]) -> io::Result<()> {
        Ok(())
    }
    fn frame(&self, _: u8) -> io::Result<Vec<u8>> {
        Err(io::ErrorKind::Unsupported.into())
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
        return Err(io::ErrorKind::Unsupported.into());
    }
    Ok(Arc::new(Sim {
        power: Mutex::new(true),
        locks: Mutex::new(json!({"num":false,"caps":false,"scroll":false})),
    }))
}
