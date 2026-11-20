//! Trusted local display connection and the shared one-second telemetry sampler.
pub mod metrics;

use crate::{console::now_ms, system, validate, ApiError, App};
use futures_util::StreamExt;
use metrics::{ReportDecoder, Samples, TextReportDecoder};
use serde_json::{json, Value};
use std::{
    fs, io,
    net::SocketAddr,
    os::unix::fs::{FileTypeExt, PermissionsExt},
    path::Path,
    sync::{atomic::Ordering, Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, UnixListener, UnixStream},
    sync::{broadcast, watch},
};
use tokio_util::codec::{FramedRead, LinesCodec};

fn read_usb() -> Option<String> {
    fs::read_dir("/sys/bus/usb/devices")
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name();
            let name = name.to_str()?;
            if name.contains(':') || !name.contains('-') {
                return None;
            }
            fs::read_to_string(e.path().join("product"))
                .ok()
                .map(|s| s.trim().to_owned())
        })
        .next()
}
fn gadget_connected() -> bool {
    fs::read_dir("/sys/class/udc").ok().is_some_and(|entries| {
        entries.flatten().any(|d| {
            fs::read_to_string(d.path().join("state")).is_ok_and(|s| s.trim() == "configured")
        })
    })
}
async fn status(app: &App, pc: bool) -> io::Result<Value> {
    let app = app.clone();
    tokio::task::spawn_blocking(move || sample_status(&app, pc))
        .await
        .map_err(io::Error::other)
}
fn sample_status(app: &App, pc: bool) -> Value {
    let info = system::device_info(app);
    let speed = info["link_mbps"].as_u64().unwrap_or_default();
    let ip = info["ip"]
        .as_str()
        .filter(|s| !s.is_empty() && *s != "0.0.0.0");
    let link = if speed == 0 {
        "down"
    } else if ip.is_none() {
        "no_ip"
    } else {
        "up"
    };
    let (hid, usb_a) = if app.sim {
        (pc, Some("USB Flash Disk".to_owned()))
    } else {
        (gadget_connected(), read_usb())
    };
    let pc_state = match (hid, pc) {
        (true, true) => "ok",
        (false, false) => "off",
        _ => "partial",
    };
    json!({"t":"status","eth":{"state":link,"mbps":speed,"ip":ip},"pc":{"state":pc_state},"usb_a":{"device":usb_a},
        "kvm":{"enabled":app.config()["kvm"]["enabled"],"sessions":app.console.session_count()},"lux":info["lux"]})
}

#[derive(Clone, Default)]
struct Host {
    samples: Samples,
    last_seen: Option<(Instant, u64)>,
}
impl Host {
    fn ingest(&mut self, id: String, value: Option<f64>, now: Instant, ts: u64) {
        if !self.online(now) {
            self.samples.clear();
        }
        self.samples.insert(id, value);
        self.last_seen = Some((now, ts));
    }

    fn online(&self, now: Instant) -> bool {
        self.last_seen
            .is_some_and(|(last, _)| now.saturating_duration_since(last) < Duration::from_secs(3))
    }
}

// f_hid keeps the latest EP0 SET_REPORT. Read it independently of the one-second display cadence.
async fn collect(app: App, host: Arc<Mutex<Host>>) {
    let mut poll = tokio::time::interval(Duration::from_millis(8));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        poll.tick().await;
        let hw = app.hw.clone();
        if let Ok(Ok(Some(raw))) = tokio::task::spawn_blocking(move || hw.telemetry()).await {
            if let Some((id, value)) = TextReportDecoder.decode(&raw) {
                host.lock()
                    .unwrap()
                    .ingest(id, value, Instant::now(), now_ms());
            }
        }
    }
}

fn brightness(config: &Value, lux: Option<f64>, offline: Option<Duration>) -> u64 {
    let sleep = config["offline_sleep_min"].as_u64().unwrap_or(5);
    if sleep > 0 && offline.is_some_and(|elapsed| elapsed >= Duration::from_secs(sleep * 60)) {
        return 0;
    }
    let manual = config["brightness"].as_u64().unwrap_or(70);
    if config["auto_brightness"] == true {
        if let Some(lux) = lux.filter(|v| v.is_finite() && *v >= 0.) {
            // A logarithmic curve gives dim indoor environments the most adjustment range.
            return (10. + 90. * lux.ln_1p() / 1000f64.ln_1p())
                .round()
                .clamp(10., 100.) as u64;
        }
    }
    manual
}

struct Stop(tokio::task::JoinHandle<()>);
impl Drop for Stop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn sampler(app: App, tx: broadcast::Sender<Value>, pc: watch::Sender<bool>) {
    let samples = Arc::new(Mutex::new(Host::default()));
    let _collector = (!app.sim).then(|| Stop(tokio::spawn(collect(app.clone(), samples.clone()))));
    let mut was_connected = app.sim && app.console.sim_pc_connected.load(Ordering::Relaxed);
    let mut offline_since = None;
    let mut previous_brightness = None;
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_status = Value::Null;
    let mut last_status_time = None;
    loop {
        tick.tick().await;
        let now = Instant::now();
        let ts = now_ms();
        let host = {
            let mut host = samples.lock().unwrap();
            if app.sim && app.console.sim_pc_connected.load(Ordering::Relaxed) {
                host.last_seen = Some((now, ts));
            }
            host.clone()
        };
        let connected = if app.sim {
            app.console.sim_pc_connected.load(Ordering::Relaxed)
        } else {
            host.online(now)
        };
        if connected != was_connected {
            was_connected = connected;
            pc.send_replace(connected);
            let _=tx.send(json!({"t":"pc","connected":connected,"last_seen_ts":host.last_seen.map(|(_,ts)|ts)}));
        }
        let Ok(state) = status(&app, connected).await else {
            return;
        };
        let panel = &app.config()["panel"];
        if !app.sim {
            if connected {
                offline_since = None;
            } else {
                offline_since.get_or_insert(now);
            }
            let level = brightness(
                panel,
                state["lux"].as_f64(),
                offline_since.map(|start| now.duration_since(start)),
            );
            if previous_brightness != Some(level) {
                let hw = app.hw.clone();
                if matches!(
                    tokio::task::spawn_blocking(move || hw.brightness(level)).await,
                    Ok(Ok(()))
                ) {
                    previous_brightness = Some(level);
                }
            }
        }
        if state != last_status
            || last_status_time
                .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(5))
        {
            let _ = tx.send(state.clone());
            last_status = state;
            last_status_time = Some(now);
        }
        if connected {
            let samples = if app.sim { None } else { Some(&host.samples) };
            let metrics = metrics::snapshot(app.started.elapsed().as_secs_f64(), panel, samples);
            let _ = tx.send(json!({"t":"telemetry","ts":ts,"metrics":metrics}));
        }
    }
}

async fn reply(writer: &mut (impl AsyncWrite + Unpin), msg: Value) -> io::Result<()> {
    let mut line = serde_json::to_vec(&msg)?;
    line.push(b'\n');
    writer.write_all(&line).await
}

fn request(app: &App, m: Value) -> Value {
    let mut out = json!({"t":"result","ok":false});
    if m["id"]
        .as_f64()
        .is_some_and(|id| id >= 0. && id.fract() == 0.)
    {
        out["id"] = m["id"].clone();
    }
    if validate("FromPanel", &m).is_err() {
        out["error"] = "bad_message".into();
        return out;
    }
    let result: Result<Option<Value>, ApiError> = match m["t"].as_str() {
        Some("get_config") => Ok(Some((*app.config()).clone())),
        Some("patch_config") => app
            .patch_config(&m["patch"], None)
            .map(|cfg| Some((*cfg).clone())),
        Some("device_info") => Ok(Some(system::device_info(app))),
        Some("action") => match m["action"].as_str().unwrap() {
            "eject_media" => app.storage.eject().map(|()| {
                app.emit("status_changed");
                None
            }),
            "usb_apply" => app.apply_usb().map(|()| None),
            action => system::device_action(app, action).map(|()| None),
        },
        _ => Err(ApiError::new(400, "bad_message")),
    };
    match result {
        Ok(data) => {
            out["ok"] = true.into();
            if let Some(d) = data {
                out["data"] = d;
            }
        }
        Err(err) => {
            out["error"] = err.code.into();
            if let Some(d) = err.detail {
                out["detail"] = d;
            }
        }
    }
    out
}

async fn client<T: AsyncRead + AsyncWrite + Unpin>(
    app: App,
    io: T,
    mut telemetry: broadcast::Receiver<Value>,
    pc: watch::Receiver<bool>,
) -> io::Result<()> {
    let mut closes = app.socket_closes.subscribe();
    let mut applies = tokio::task::JoinSet::new();
    let mut events = app.events.subscribe();
    let (initial_config, mut config_updates) = app.subscribe_config();
    let (read, mut write) = tokio::io::split(io);
    let mut lines = FramedRead::new(read, LinesCodec::new_with_max_length(1024 * 1024));
    reply(
        &mut write,
        json!({"t":"hello","version":env!("CARGO_PKG_VERSION")}),
    )
    .await?;
    reply(&mut write, json!({"t":"config","config":initial_config})).await?;
    let connected = *pc.borrow();
    let initial = status(&app, connected).await?;
    reply(&mut write, initial).await?;
    loop {
        tokio::select! {
            biased;
            changed=closes.changed()=>{
                if changed.is_err(){return Ok(())}
                let code=*closes.borrow_and_update();
                if code==1012{
                    reply(&mut write,json!({"t":"kvm_restarting"})).await?;
                    return write.shutdown().await;
                }
            },
            event=events.recv()=>match event{
                Ok(event) if event!="console_sessions"=>{},
                Err(broadcast::error::RecvError::Closed)=>return Ok(()),
                other=>{
                    if other.is_err(){events=events.resubscribe();}
                    let connected=*pc.borrow();
                    reply(&mut write,status(&app,connected).await?).await?;
                }
            },
            update=config_updates.recv()=>{
                let config=match update{
                    Ok(cfg)=>cfg,
                    Err(broadcast::error::RecvError::Lagged(_))=>{
                        let (cfg,rx)=app.subscribe_config();
                        config_updates=rx;
                        cfg
                    }
                    Err(broadcast::error::RecvError::Closed)=>return Ok(()),
                };
                reply(&mut write,json!({"t":"config","config":config})).await?;
                let connected=*pc.borrow();
                reply(&mut write,status(&app,connected).await?).await?;
            },
            Some(out)=applies.join_next(), if !applies.is_empty()=>reply(&mut write,out.map_err(io::Error::other)?).await?,
            next=lines.next(), if applies.len()<16=>match next{
                Some(Ok(line))=>{
                    let msg:Value=serde_json::from_str(&line).unwrap_or(Value::Null);
                    if msg["t"]=="get_config" {
                        reply(&mut write,request(&app,msg)).await?;
                    } else if msg["t"]=="action" && msg["action"]=="usb_apply" {
                        // The request ID lets this peer query the config while re-enumeration runs.
                        let app=app.clone();applies.spawn_blocking(move||request(&app,msg));
                    }else{
                        let app=app.clone();
                        let out=tokio::task::spawn_blocking(move||request(&app,msg)).await.map_err(io::Error::other)?;
                        reply(&mut write,out).await?;
                    }
                },
                Some(Err(_))=>reply(&mut write,json!({"t":"result","ok":false,"error":"bad_message"})).await?,
                None=>return Ok(())
            },
            msg=telemetry.recv()=>match msg{
                Ok(data)=>reply(&mut write,data).await?,
                Err(broadcast::error::RecvError::Lagged(_))=>continue,
                Err(_)=>return Ok(()),
            }
        }
    }
}

enum Listener {
    Unix(UnixListener),
    Tcp(TcpListener),
}
impl Listener {
    async fn bind(addr: &str) -> io::Result<Self> {
        if addr.starts_with('/') {
            let path = Path::new(addr);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            if let Ok(meta) = fs::symlink_metadata(path) {
                if !meta.file_type().is_socket() {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "panel path is occupied",
                    ));
                }
                match UnixStream::connect(path).await {
                    Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
                        fs::remove_file(path)?
                    }
                    Err(e) => return Err(e),
                    Ok(_) => {
                        return Err(io::Error::new(
                            io::ErrorKind::AddrInUse,
                            "panel socket already active",
                        ))
                    }
                }
            }
            let listener = UnixListener::bind(path)?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o660))?;
            Ok(Self::Unix(listener))
        } else {
            let socket: SocketAddr = addr.parse().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "invalid panel address")
            })?;
            if !socket.ip().is_loopback() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "trusted panel TCP listener requires loopback",
                ));
            }
            Ok(Self::Tcp(TcpListener::bind(socket).await?))
        }
    }
}

pub async fn serve(app: App, addr: &str) -> io::Result<()> {
    let listener = Listener::bind(addr).await?;
    let (messages, _) = broadcast::channel(32);
    let (pc, connected) =
        watch::channel(app.sim && app.console.sim_pc_connected.load(Ordering::Relaxed));
    let worker = tokio::spawn(sampler(app.clone(), messages.clone(), pc));
    let _sampler = Stop(worker);
    loop {
        match &listener {
            Listener::Unix(listener) => {
                let (s, _) = listener.accept().await?;
                tokio::spawn(client(
                    app.clone(),
                    s,
                    messages.subscribe(),
                    connected.clone(),
                ));
            }
            Listener::Tcp(listener) => {
                let (s, _) = listener.accept().await?;
                tokio::spawn(client(
                    app.clone(),
                    s,
                    messages.subscribe(),
                    connected.clone(),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn host_gaps_clear_old_metrics_before_reconnect() {
        let now = Instant::now();
        let mut h = Host::default();
        h.ingest("cpu.tctl".into(), Some(60.), now, 1000);
        h.ingest(
            "gpu.temp".into(),
            Some(50.),
            now + Duration::from_millis(10),
            1010,
        );
        assert!(h.online(now + Duration::from_secs(3)));
        let later = now + Duration::from_secs(4);
        assert!(!h.online(later));
        h.ingest("cpu.tctl".into(), Some(61.), later, 5000);
        assert_eq!(h.samples.len(), 1);
        assert_eq!(h.samples["cpu.tctl"], Some(61.));
        assert_eq!(h.last_seen.unwrap().1, 5000);
    }
    #[test]
    fn backlight_tracks_lux_manual_override_timeout_and_reconnection() {
        let mut cfg = json!({"auto_brightness":true,"brightness":63,"offline_sleep_min":5});
        assert_eq!(brightness(&cfg, Some(0.), None), 10);
        let office = brightness(&cfg, Some(100.), None);
        assert!((10..100).contains(&office));
        assert_eq!(brightness(&cfg, Some(2000.), None), 100);
        assert_eq!(brightness(&cfg, None, None), 63);
        cfg["auto_brightness"] = false.into();
        assert_eq!(
            brightness(&cfg, Some(100.), Some(Duration::from_secs(299))),
            63
        );
        assert_eq!(
            brightness(&cfg, Some(100.), Some(Duration::from_secs(300))),
            0
        );
        assert_eq!(brightness(&cfg, Some(100.), None), 63);
        cfg["offline_sleep_min"] = 0.into();
        assert_eq!(brightness(&cfg, None, Some(Duration::from_secs(1000))), 63);
    }
}
