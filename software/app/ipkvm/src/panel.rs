//! Trusted local display connection and the shared one-second telemetry sampler.
mod board;
pub mod metrics;

use crate::{console::now_ms, system, validate, ApiError, App};
use futures_util::StreamExt;
use metrics::{DeviceNames, Samples, VendorReport};
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
    tokio::task::spawn_blocking(move || sample_status(&app, pc, &system::device_info(&app)))
        .await
        .map_err(io::Error::other)
}
pub(crate) fn mcp_metric_catalogue(app: &App) -> Vec<Value> {
    let mut config = app.config()["panel"].clone();
    config["hidden_metrics"] = json!([]);
    metrics::snapshot(0.0, &config, None, None)
}

pub(crate) fn mcp_metric(app: &App, id: &str, reading: Option<f64>) -> Option<Value> {
    let mut config = app.config()["panel"].clone();
    config["hidden_metrics"] = json!([]);
    let values = [(id.to_owned(), reading)].into();
    metrics::snapshot(0.0, &config, Some(&values), None)
        .into_iter()
        .next()
}

fn sample_status(app: &App, pc: bool, info: &Value) -> Value {
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
    session: Option<(u64, Duration)>,
    received: std::collections::BTreeMap<String, Instant>,
    names: DeviceNames,
}
impl Host {
    fn ingest(&mut self, id: String, value: Option<f64>, now: Instant, ts: u64) {
        if !self.online(now) {
            *self = Self::default();
        }
        const MAX_METRICS: usize = 768;
        if self.samples.len() >= MAX_METRICS && !self.samples.contains_key(&id) {
            return;
        }
        // Slot labels in vendor reports are dynamic. Bound their retained inventory
        // independently of the fixed CPU/GPU metrics on the 512 MiB board.
        const MAX_DIMMS: usize = 256;
        if id.starts_with("mem.dimm.")
            && !self.samples.contains_key(&id)
            && self
                .samples
                .keys()
                .filter(|key| key.starts_with("mem.dimm."))
                .count()
                >= MAX_DIMMS
        {
            return;
        }
        if self.session.is_some() {
            self.received.insert(id.clone(), now);
        }
        self.samples.insert(id, value);
        self.last_seen = Some((now, ts));
    }

    fn report(&mut self, report: VendorReport, now: Instant, ts: u64) {
        match report {
            VendorReport::Hello(id, interval) => {
                if !self.online(now) || self.session.is_none_or(|(current, _)| current != id) {
                    *self = Self::default();
                }
                self.session = Some((id, Duration::from_millis(interval)));
                self.last_seen = Some((now, ts));
            }
            VendorReport::DeviceName(id, name) if self.online(now) && self.session.is_some() => {
                if self.names.len() < 768 || self.names.contains_key(&id) {
                    self.names.insert(id, name);
                }
            }
            VendorReport::Values(items) => {
                for (id, value) in items {
                    self.ingest(id, value, now, ts);
                }
            }
            _ => {}
        }
    }

    fn snapshot(&mut self) -> Self {
        self.snapshot_at(Instant::now())
    }

    fn snapshot_at(&mut self, now: Instant) -> Self {
        if let Some((_, interval)) = self.session {
            let freshness = (interval * 2 + Duration::from_secs(1)).max(Duration::from_secs(4));
            self.received.retain(|id, timestamp| {
                let age = now.saturating_duration_since(*timestamp);
                if age >= freshness * 2 {
                    self.samples.remove(id);
                    return false;
                }
                if age >= freshness {
                    self.samples.insert(id.clone(), None);
                }
                true
            });
            return self.clone();
        }
        let captured = self.clone();
        // Preserve the reported inventory. A missed sample in the next second
        // is null, rather than displaying the previous measurement as fresh.
        self.samples.values_mut().for_each(|sample| *sample = None);
        captured
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
        let request_app = app.clone();
        if let Ok(Ok(Some(raw))) = tokio::task::spawn_blocking(move || {
            let raw = hw.telemetry()?;
            if let Some(raw) = &raw {
                request_app.mcp.host.packet(&request_app, raw);
            }
            Ok::<_, io::Error>(raw)
        })
        .await
        {
            if let Some(report) = metrics::vendor_report(&raw) {
                match &report {
                    VendorReport::Hello(session, _) => app.mcp.host.heartbeat(*session),
                    VendorReport::Values(items) => app.mcp.metrics.received(items),
                    _ => {}
                }
                host.lock()
                    .unwrap()
                    .report(report, Instant::now(), now_ms());
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
    let mut board = board::BoardSampler::default();
    loop {
        tick.tick().await;
        let now = Instant::now();
        let ts = now_ms();
        let host = {
            let mut host = samples.lock().unwrap();
            if app.sim && app.console.sim_pc_connected.load(Ordering::Relaxed) {
                host.last_seen = Some((now, ts));
            }
            host.snapshot()
        };
        let connected = if app.sim {
            app.console.sim_pc_connected.load(Ordering::Relaxed)
        } else {
            host.online(now)
        };
        app.mcp
            .metrics
            .connection(connected, host.session.map(|(id, _)| id));
        if connected != was_connected {
            was_connected = connected;
            pc.send_replace(connected);
            let _=tx.send(json!({"t":"pc","connected":connected,"last_seen_ts":host.last_seen.map(|(_,ts)|ts)}));
        }
        let poll_app = app.clone();
        let Ok((next_board, state, mut board_state)) = tokio::task::spawn_blocking(move || {
            let info = system::device_info(&poll_app);
            let state = sample_status(&poll_app, connected, &info);
            let board_state = board.sample(&poll_app, &info, now, ts);
            (board, state, board_state)
        })
        .await
        else {
            return;
        };
        board = next_board;
        let panel = &app.config()["panel"];
        let mut asleep = false;
        if !app.sim {
            if connected || app.mcp.activity()["active"] == true {
                offline_since = None;
            } else {
                offline_since.get_or_insert(now);
            }
            let activity = *app.panel_activity.lock().unwrap();
            let idle =
                offline_since.map(|start| now.saturating_duration_since(start.max(activity)));
            let sleep = panel["offline_sleep_min"].as_u64().unwrap_or(5);
            asleep = sleep > 0 && idle.is_some_and(|idle| idle >= Duration::from_secs(sleep * 60));
            let level = brightness(panel, state["lux"].as_f64(), idle);
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
        board_state["display_asleep"] = json!(asleep);
        let _ = tx.send(board_state);
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
            let mut catalogue_settings = panel.clone();
            catalogue_settings["hidden_metrics"] = json!([]);
            let mut metrics = metrics::snapshot(
                app.started.elapsed().as_secs_f64(),
                &catalogue_settings,
                samples,
                Some(&host.names),
            );
            app.mcp.metrics.snapshot(metrics.clone(), app.sim);
            let catalogue = metrics.iter().map(|metric| {
                json!({"id":metric["id"],"label":metric["label"],"unit":metric["unit"],"group":metric["group"]})
            }).collect();
            *app.panel_metrics.write().unwrap() = catalogue;
            if let Some(hidden) = panel["hidden_metrics"].as_array() {
                metrics.retain(|metric| !hidden.contains(&metric["id"]));
            }
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
        Some("metric_catalogue") => Ok(Some(json!(*app.panel_metrics.read().unwrap()))),
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
            "pc_power_on" => crate::console::panel_power_on(app).map(|()| None),
            "wake_display" => {
                *app.panel_activity.lock().unwrap() = Instant::now();
                Ok(None)
            }
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
    let mut previous_mcp = Value::Null;
    let mut mcp_tick = tokio::time::interval(Duration::from_millis(150));
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
            _=mcp_tick.tick()=>{
                let mcp = app.mcp.activity();
                if mcp != previous_mcp {
                    previous_mcp = mcp.clone();
                    let mut msg = mcp;
                    msg["t"] = "mcp".into();
                    reply(&mut write,msg).await?;
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
                    if msg["t"]=="mcp_stop" && validate("FromPanel", &msg).is_ok() {
                        let result = crate::mcp::stop_from_function_key(&app).await;
                        let mut out = json!({"t":"result","ok":result.is_ok()});
                        if let Some(id) = msg.get("id") { out["id"] = id.clone(); }
                        if let Err(error) = result { out["error"] = error.code.into(); }
                        reply(&mut write,out).await?;
                    } else if msg["t"]=="get_config" {
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
    use super::metrics::{ReportDecoder, TextReportDecoder};
    use super::*;

    #[test]
    fn usb_session_keeps_slow_junction_readings_and_clears_old_devices() {
        fn packet(payload: &str) -> VendorReport {
            let mut bytes = [0; 64];
            bytes[0] = 3;
            bytes[1..1 + payload.len()].copy_from_slice(payload.as_bytes());
            metrics::vendor_report(&bytes).unwrap()
        }
        let now = Instant::now();
        let mut host = Host::default();
        host.report(packet("!0000000000000001:10000"), now, 0);
        host.report(packet(":gpu.1234abcd1234abcd=RTX 5090"), now, 0);
        host.report(
            packet("gpu.1234abcd1234abcd.mem_junction=61\nmem.used=18"),
            now,
            0,
        );
        let captured = host.snapshot_at(now + Duration::from_secs(5));
        let readings = metrics::snapshot(
            0.,
            &json!({}),
            Some(&captured.samples),
            Some(&captured.names),
        );
        assert!(readings
            .iter()
            .any(|metric| metric["id"] == "gpu.1234abcd1234abcd.mem_junction"
                && metric["label"] == "RTX 5090 · Memory junction"
                && metric["value"] == 61.0
                && metric["unit"] == "°C"));
        assert_eq!(captured.samples["mem.used"], Some(18.0));

        let later = now + Duration::from_secs(22);
        host.report(packet("!0000000000000001:10000"), later, 22_000);
        // The host was offline, so a new heartbeat alone must not revive stale measurements.
        assert!(host.snapshot_at(later).samples.is_empty());
        host.report(
            packet("gpu.1234abcd1234abcd.mem_junction=62"),
            later,
            22_000,
        );
        host.report(packet("!0000000000000002:10000"), later, 22_000);
        assert!(host.snapshot_at(later).samples.is_empty());
    }
    #[test]
    fn missing_metrics_are_omitted_but_reported_slots_survive_a_missing_read() {
        let mut host = Host::default();
        let mut report = [0; 64];
        report[0] = 3;
        let payload = b"mem.dimm.c1=46.5";
        report[1..1 + payload.len()].copy_from_slice(payload);
        let (id, value) = TextReportDecoder.decode(&report).unwrap();
        host.ingest(id, value, Instant::now(), 1000);
        for expected in [json!(46.5), Value::Null] {
            let m = metrics::snapshot(0., &json!({}), Some(&host.snapshot().samples), None);
            assert_eq!(m.len(), 1);
            assert_eq!(m[0]["id"], "mem.dimm.c1");
            assert_eq!(m[0]["value"], expected);
        }
    }
    #[test]
    fn reported_null_keeps_panel_row_until_a_new_value_arrives() {
        let packet = |payload: &[u8]| {
            let mut report = [0; 64];
            report[0] = 3;
            report[1..1 + payload.len()].copy_from_slice(payload);
            metrics::vendor_report(&report).unwrap()
        };
        let start = Instant::now();
        let mut host = Host::default();
        for second in 0..=12 {
            let now = start + Duration::from_secs(second);
            host.report(packet(b"!0000000000000001:1000"), now, 1000 * second);
            let (payload, expected) = match second {
                1 => (b"cpu.bclk=100".as_slice(), json!(100.0)),
                12 => (b"cpu.bclk=101".as_slice(), json!(101.0)),
                _ => (b"cpu.bclk=null".as_slice(), Value::Null),
            };
            host.report(packet(payload), now, 1000 * second);
            let readings =
                metrics::snapshot(0., &json!({}), Some(&host.snapshot_at(now).samples), None);
            assert_eq!(readings.len(), 1);
            assert_eq!(readings[0]["id"], "cpu.bclk");
            assert_eq!(readings[0]["value"], expected);
        }
    }
    #[test]
    fn repeated_new_slot_names_cannot_grow_the_inventory_indefinitely() {
        let mut host = Host::default();
        let now = Instant::now();
        for i in 0..300 {
            host.ingest(format!("mem.dimm.slot_{i}"), Some(40.), now, 1);
        }
        host.ingest("cpu.tctl".into(), Some(60.), now, 1);
        assert_eq!(host.samples.len(), 257);
        assert_eq!(host.samples["cpu.tctl"], Some(60.));
    }
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
