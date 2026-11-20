//! Apply persisted board settings on startup, on changes from either UI, and
//! again when an interface, daemon or device node was unavailable.
use crate::App;
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::{fd::AsRawFd, unix::fs::FileTypeExt},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{process::Command, sync::broadcast, time};

fn update_error(previous: &mut Option<String>, error: Option<String>) {
    if *previous != error {
        if let Some(message) = &error {
            eprintln!("kvmd device settings: {message}");
        }
        *previous = error;
    }
}

// Buildroot places its normal start/stop helpers outside /etc/init.d so even
// before kvmd loads the saved configuration, disabled daemons stay stopped.
fn running(root: &Path, name: &str) -> io::Result<bool> {
    let text = match fs::read_to_string(root.join(format!("var/run/{name}.pid"))) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    let Some(pid) = text.trim().parse::<u32>().ok().filter(|p| *p > 1) else {
        return Ok(false);
    };
    let exe = match fs::read_link(root.join(format!("proc/{pid}/exe"))) {
        Ok(p) => p,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    Ok(exe == fs::canonicalize(root.join("usr/sbin").join(name))?)
}

async fn daemon(root: &Path, name: &str, on: bool, restart: bool) -> io::Result<()> {
    let already = running(root, name)?;
    let action = match (on, already, restart) {
        (true, false, _) => "start",
        (true, true, true) => "restart",
        (false, true, _) => "stop",
        _ => return Ok(()),
    };
    let command = root.join("usr/libexec/kvmd").join(name);
    let status = time::timeout(
        Duration::from_secs(8),
        Command::new(command)
            .arg(action)
            .kill_on_drop(true)
            .status(),
    )
    .await??;
    if !status.success() {
        return Err(io::Error::other(format!("{name} {action}: {status}")));
    }
    // The Dropbear init helper may print FAIL yet return a successful echo.
    // Both helpers daemonise before their final pid file becomes visible.
    for _ in 0..20 {
        if running(root, name)? == on {
            return Ok(());
        }
        time::sleep(Duration::from_millis(50)).await;
    }
    Err(io::Error::other(format!("{name} did not {action}")))
}

struct Services {
    root: PathBuf,
    advertised: Option<String>,
    errors: [Option<String>; 3],
}
impl Services {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            advertised: None,
            errors: Default::default(),
        }
    }
    async fn apply(&mut self, device: &Value) {
        let Some(host) = device["hostname"].as_str() else {
            return;
        };
        let hostname = self.root.join("proc/sys/kernel/hostname");
        let result = fs::read_to_string(&hostname).and_then(|current| {
            if current.trim() != host {
                fs::write(hostname, host)?;
            }
            Ok(())
        });
        let hostname_ready = result.is_ok();
        update_error(
            &mut self.errors[0],
            result.err().map(|e| format!("hostname: {e}")),
        );
        let ssh = daemon(&self.root, "dropbear", device["ssh"] == true, false).await;
        update_error(&mut self.errors[1], ssh.err().map(|e| format!("SSH: {e}")));
        let mdns = device["mdns"] == true;
        if hostname_ready || !mdns {
            let result = daemon(
                &self.root,
                "mdnsd",
                mdns,
                self.advertised.as_deref() != Some(host),
            )
            .await;
            if result.is_ok() {
                self.advertised = mdns.then(|| host.to_owned());
            }
            update_error(
                &mut self.errors[2],
                result.err().map(|e| format!("mDNS: {e}")),
            );
        }
    }
}

async fn services(app: App) {
    let (cfg, mut changes) = app.subscribe_config();
    let mut desired = cfg["device"].clone();
    let mut board = Services::new("/".into());
    let mut retry = time::interval(Duration::from_secs(5));
    retry.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _=retry.tick()=>{},
            recv=changes.recv()=>{
                let next=match recv{
                    Ok(v)=>v,Err(broadcast::error::RecvError::Lagged(_))=>app.config(),Err(_)=>break
                };
                if next["device"]==desired {continue;}
                desired=next["device"].clone();
            }
        }
        board.apply(&desired).await;
    }
}

// The watchdog runs independently of init-script timeouts. 250 ms is below the
// shortest whole-second timeout exposed by the sunxi driver.
trait WatchdogDevice: Write {
    fn stop_timer(&self) -> io::Result<()>;
}
impl WatchdogDevice for File {
    fn stop_timer(&self) -> io::Result<()> {
        const DISABLE: i32 = 1; // WDIOS_DISABLECARD
        let status = unsafe {
            libc::ioctl(
                self.as_raw_fd(),
                libc::_IOR::<i32>(b'W' as u32, 4),
                &DISABLE,
            )
        };
        if status < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}
struct Watchdog<T: WatchdogDevice> {
    file: Option<T>,
}
impl<T: WatchdogDevice> Watchdog<T> {
    fn tick(&mut self, on: bool, open: impl FnOnce() -> io::Result<T>) -> io::Result<()> {
        if on {
            if self.file.is_none() {
                self.file = Some(open()?);
            }
            self.file.as_mut().unwrap().write_all(b"\0")?;
        } else if let Some(file) = &mut self.file {
            // An active nowayout driver must keep receiving heartbeats after
            // refusing DISABLECARD. Retain the fd and report the refusal.
            if let Err(e) = file.stop_timer() {
                file.write_all(b"\0")?;
                return Err(e);
            }
            file.write_all(b"V")?;
            self.file = None;
        }
        Ok(())
    }
}
impl<T: WatchdogDevice> Drop for Watchdog<T> {
    fn drop(&mut self) {
        if let Some(file) = &mut self.file {
            if let Err(e) = file.write_all(b"V") {
                eprintln!("watchdog magic close: {e}");
            }
        }
    }
}
async fn watchdog(app: App) {
    let (cfg, mut changes) = app.subscribe_config();
    let mut on = cfg["device"]["watchdog"] == true;
    let mut dog = Watchdog { file: None };
    let mut tick = time::interval(Duration::from_millis(250));
    tick.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    let mut error = None;
    loop {
        tokio::select! {
            _=tick.tick(), if on || dog.file.is_some()=>{},
            recv=changes.recv()=>{
                let cfg=match recv{Ok(v)=>v,Err(broadcast::error::RecvError::Lagged(_))=>app.config(),Err(_)=>break};
                let next=cfg["device"]["watchdog"]==true;
                if next==on{continue;}
                on=next;
            }
        }
        let result = dog.tick(on, || {
            let path = Path::new("/dev/watchdog0");
            if !fs::metadata(path)?.file_type().is_char_device() {
                return Err(io::Error::other("watchdog0 is not a character device"));
            }
            OpenOptions::new().write(true).open(path)
        });
        update_error(&mut error, result.err().map(|e| format!("watchdog0: {e}")));
    }
}

pub(super) fn start(app: App) {
    tokio::spawn(services(app.clone()));
    tokio::spawn(watchdog(app));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{cell::RefCell, os::unix::fs::PermissionsExt, rc::Rc};
    #[tokio::test]
    async fn reconciles_and_retries_with_fake_proc_tree_and_init_helpers() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path();
        for path in [
            "var/run",
            "usr/libexec/kvmd",
            "usr/sbin",
            "proc/sys/kernel",
            "proc/71",
            "proc/72",
        ] {
            fs::create_dir_all(dir.join(path)).unwrap();
        }
        fs::write(dir.join("proc/sys/kernel/hostname"), "old").unwrap();
        for (name, pid) in [("dropbear", 71), ("mdnsd", 72)] {
            fs::write(dir.join("usr/sbin").join(name), []).unwrap();
            let path = dir.join("usr/libexec/kvmd").join(name);
            let fixture = format!(
                r#"#!/bin/sh
root=$(cd "$(dirname "$0")/../../.." && pwd)
echo "{name} $1" >> "$root/actions"
test ! -f "$root/fail-{name}" || exit 1
case "$1" in
 start|restart)
   ln -sf "$root/usr/sbin/{name}" "$root/proc/{pid}/exe"
   echo {pid} > "$root/var/run/{name}.pid"
   ;;
 stop)
   rm -f "$root/proc/{pid}/exe" "$root/var/run/{name}.pid"
   ;;
esac
"#
            );
            fs::write(&path, fixture).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let mut svc = Services::new(dir.into());
        let mut cfg = json!({"hostname":"kvmpanel","ssh":false,"mdns":true});
        svc.apply(&cfg).await;
        assert_eq!(
            fs::read_to_string(dir.join("proc/sys/kernel/hostname")).unwrap(),
            "kvmpanel"
        );
        assert_eq!(
            fs::read_to_string(dir.join("actions")).unwrap(),
            "mdnsd start\n"
        );
        svc.apply(&cfg).await;
        cfg["hostname"] = json!("desk");
        cfg["ssh"] = json!(true);
        fs::write(dir.join("fail-dropbear"), []).unwrap();
        svc.apply(&cfg).await;
        assert!(svc.errors[1].is_some());
        fs::remove_file(dir.join("fail-dropbear")).unwrap();
        svc.apply(&cfg).await;
        assert!(svc.errors[1].is_none());
        cfg["ssh"] = false.into();
        cfg["mdns"] = false.into();
        svc.apply(&cfg).await;
        assert_eq!(fs::read_to_string(dir.join("actions")).unwrap(),"mdnsd start\ndropbear start\nmdnsd restart\ndropbear start\ndropbear stop\nmdnsd stop\n");
        assert!(!running(dir, "mdnsd").unwrap());
    }

    struct Fake {
        log: Rc<RefCell<Vec<u8>>>,
        nowayout: bool,
    }
    impl Write for Fake {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            self.log.borrow_mut().extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl WatchdogDevice for Fake {
        fn stop_timer(&self) -> io::Result<()> {
            if self.nowayout {
                Err(io::Error::from_raw_os_error(libc::EBUSY))
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn watchdog_only_opens_when_enabled_and_magic_closes() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut dog = Watchdog::<Fake> { file: None };
        dog.tick(false, || panic!("disabled at boot")).unwrap();
        dog.tick(true, || {
            Ok(Fake {
                log: log.clone(),
                nowayout: true,
            })
        })
        .unwrap();
        assert!(dog.tick(false, || panic!()).is_err());
        assert!(dog.file.is_some());
        dog.file.as_mut().unwrap().nowayout = false;
        dog.tick(false, || panic!()).unwrap();
        assert!(dog.file.is_none());
        dog.tick(true, || {
            Ok(Fake {
                log: log.clone(),
                nowayout: false,
            })
        })
        .unwrap();
        drop(dog);
        assert_eq!(&*log.borrow(), b"\0\0V\0V");
    }
}
