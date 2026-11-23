use super::*;
use crate::console::video::{self, Frame};
use axum::body::Bytes;
use base64::engine::general_purpose::STANDARD;
use std::collections::VecDeque;

const FULL_REGION: [f64; 4] = [0.0, 0.0, 1.0, 1.0];
const HISTORY_TIME: Duration = Duration::from_secs(600);
const HISTORY_BYTES: usize = 8 * 1024 * 1024;

#[derive(Default)]
struct MetricState {
    online: bool,
    session: Option<u64>,
    catalogue: BTreeMap<String, Value>,
    current: Vec<Value>,
}

type Samples = Vec<(String, Option<f64>)>;

pub(crate) struct Telemetry {
    state: Mutex<MetricState>,
    samples: tokio::sync::broadcast::Sender<Samples>,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self {
            state: Mutex::default(),
            samples: tokio::sync::broadcast::channel(128).0,
        }
    }
}

impl Telemetry {
    pub(crate) fn received(&self, items: &[(String, Option<f64>)]) {
        let _ = self.samples.send(items.to_vec());
    }
    pub(crate) fn connection(&self, online: bool, session: Option<u64>) {
        let mut state = self.state.lock().unwrap();
        if online != state.online || state.session != session {
            state.current.clear();
        }
        state.online = online;
        state.session = session;
    }
    pub(crate) fn snapshot(&self, metrics: Vec<Value>, simulated: bool) {
        if simulated {
            self.received(
                &metrics
                    .iter()
                    .map(|v| (v["id"].as_str().unwrap().into(), v["value"].as_f64()))
                    .collect::<Vec<_>>(),
            );
        }
        let mut state = self.state.lock().unwrap();
        for item in &metrics {
            let mut entry = item.clone();
            let fields = entry.as_object_mut().unwrap();
            fields.remove("value");
            fields.remove("warn");
            state
                .catalogue
                .insert(item["id"].as_str().unwrap().into(), entry);
        }
        if state.catalogue.len() > 768 {
            let known = metrics
                .iter()
                .filter_map(|metric| metric["id"].as_str())
                .collect::<BTreeSet<_>>();
            state.catalogue.retain(|id, _| known.contains(id.as_str()));
        }
        state.current = metrics;
    }
    pub(crate) fn session(&self, app: &App) -> Option<u64> {
        if app.sim {
            return app.mcp.host.session(app);
        }
        let state = self.state.lock().unwrap();
        state.online.then_some(state.session).flatten()
    }
    fn online(&self, app: &App) -> bool {
        if app.sim {
            app.mcp.host.status(app)["online"] == true
        } else {
            self.state.lock().unwrap().online
        }
    }
    fn catalogue(&self, app: &App) -> Value {
        let state = self.state.lock().unwrap();
        let mut items: BTreeMap<_, _> = crate::panel::mcp_metric_catalogue(app)
            .into_iter()
            .map(|mut row| {
                let id = row["id"].as_str().unwrap().to_owned();
                let fields = row.as_object_mut().unwrap();
                fields.remove("value");
                fields.remove("warn");
                (id, row)
            })
            .collect();
        items.extend(state.catalogue.clone());
        json!({"metrics":items.into_values().collect::<Vec<_>>()})
    }
}

fn select(args: &Value, id: &str, group: &str) -> bool {
    if let Some(ids) = args["ids"].as_array() {
        return ids.iter().filter_map(Value::as_str).any(|pattern| {
            pattern
                .strip_suffix('*')
                .map_or(id == pattern, |prefix| id.starts_with(prefix))
        });
    }
    args["groups"]
        .as_array()
        .is_none_or(|groups| groups.iter().any(|v| v == group))
}

fn current(app: &App, args: &Value) -> Vec<Value> {
    app.mcp.metrics.state.lock().unwrap().current.iter().filter(|m| select(args,m["id"].as_str().unwrap(),m["group"].as_str().unwrap())).map(|v| json!({
        "id":v["id"],"label":v["label"],"unit":v["unit"],"value":v["value"],"group":v["group"],"warn":v["warn"]
    })).collect()
}

#[derive(Default)]
struct Statistics {
    min: Option<f64>,
    max: Option<f64>,
    sum: f64,
    samples: u64,
    nulls: u64,
    last: Option<f64>,
    warn: Option<f64>,
    over_warn: u64,
}
impl Statistics {
    fn ingest(&mut self, reading: Option<f64>) {
        self.last = reading;
        if let Some(value) = reading {
            self.samples += 1;
            self.min = Some(self.min.map_or(value, |old| old.min(value)));
            self.max = Some(self.max.map_or(value, |old| old.max(value)));
            self.sum += value;
            if self.warn.is_some_and(|warn| value > warn) {
                self.over_warn += 1;
            }
        } else {
            self.nulls += 1;
        }
    }
    fn json(&self, id: &str) -> Value {
        json!({"id":id,"min":self.min,"max":self.max,"avg":(self.samples > 0).then(|| self.sum / self.samples as f64),
               "last":self.last,"samples":self.samples,"nulls":self.nulls,"over_warn":self.over_warn})
    }
}

async fn watch_metrics(app: &App, args: &Value, permit: &Permit) -> Result<Value, ApiError> {
    if !app.mcp.metrics.online(app) {
        return Err(ApiError::new(503, "telemetry_offline"));
    }
    let mut rx = app.mcp.metrics.samples.subscribe();
    let mut stats: BTreeMap<String, Statistics> = current(app, args)
        .iter()
        .map(|row| {
            (
                row["id"].as_str().unwrap().into(),
                Statistics {
                    last: row["value"].as_f64(),
                    warn: row["warn"].as_f64(),
                    ..Statistics::default()
                },
            )
        })
        .collect();
    let seconds = args["seconds"].as_u64().unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    loop {
        let values = tokio::select! {
            biased;
            error = permit.interrupted() => return Err(error),
            _ = tokio::time::sleep_until(deadline) => break,
            samples = rx.recv() => samples,
        };
        match values {
            Ok(values) => {
                for (id, value) in values {
                    if let Some(stat) = stats.get_mut(&id) {
                        stat.ingest(value);
                    } else if let Some(row) = crate::panel::mcp_metric(app, &id, value)
                        .filter(|row| select(args, &id, row["group"].as_str().unwrap()))
                    {
                        let mut stat = Statistics {
                            warn: row["warn"].as_f64(),
                            ..Statistics::default()
                        };
                        stat.ingest(value);
                        stats.insert(id, stat);
                    }
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                return Err(ApiError::new(503, "telemetry_overrun"))
            }
            Err(_) => break,
        }
    }
    Ok(json!({"metrics":stats.into_iter().map(|(id, value)| value.json(&id)).collect::<Vec<_>>()}))
}

#[derive(Default)]
pub(super) struct History(Mutex<VecDeque<(Instant, u64, Bytes)>>);

fn data_image(jpeg: &[u8]) -> Value {
    json!({"type":"image","mimeType":"image/jpeg","data":STANDARD.encode(jpeg)})
}

pub(super) async fn screenshot(app: &App, args: &Value) -> Result<Output, ApiError> {
    let jpeg = Bytes::from(crate::console::snapshot(app).await?);
    rendered(
        jpeg,
        args.get("region").cloned(),
        args["max_width"].as_u64().unwrap_or(1280),
        args["quality"].as_u64().map(|n| n as u8),
    )
    .await
}

pub(super) async fn rendered(
    jpeg: Bytes,
    region: Option<Value>,
    width: u64,
    quality: Option<u8>,
) -> Result<Output, ApiError> {
    let (jpeg, structured) =
        tokio::task::spawn_blocking(move || image::render(jpeg, region.as_ref(), width, quality))
            .await
            .map_err(io::Error::other)??;
    Ok(Output {
        structured,
        content: vec![data_image(&jpeg)],
    })
}

fn region_for_result(region: Option<&Value>) -> Value {
    region.cloned().unwrap_or_else(
        || json!({"x":FULL_REGION[0],"y":FULL_REGION[1],"w":FULL_REGION[2],"h":FULL_REGION[3]}),
    )
}

pub(super) async fn wait_for_frame(
    app: &App,
    args: &Value,
    permit: &Permit,
) -> Result<Output, ApiError> {
    let started = Instant::now();
    let timeout = Duration::from_millis(args["timeout_ms"].as_u64().unwrap_or(30_000));
    let stable = Duration::from_millis(args["stable_ms"].as_u64().unwrap_or(1500));
    let threshold = args["threshold"].as_f64().unwrap_or(0.005);
    let changed = args["until"] == "changed";
    let region = args.get("region").cloned();
    let mut frames = video::subscribe(app, true).await;
    frames.rx.mark_changed();
    let mut last = None;
    let mut reference: Option<Vec<u8>> = None;
    let mut stable_since = Instant::now();
    let mut met = false;
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        tokio::select! {
            biased;
            error = permit.interrupted() => return Err(error),
            _ = tokio::time::sleep_until(deadline) => break,
            update = frames.rx.changed() => if update.is_err() { break; },
        }
        let item = frames.rx.borrow_and_update().clone();
        let Frame::Jpeg(jpeg) = item else {
            if !changed {
                reference = None;
            }
            continue;
        };
        let input = jpeg.clone();
        let area = region.clone();
        let luma =
            tokio::task::spawn_blocking(move || image::difference_frame(&input, area.as_ref()))
                .await
                .map_err(io::Error::other)??;
        last = Some(jpeg);
        let Some(previous) = reference.as_ref() else {
            reference = Some(luma);
            stable_since = Instant::now();
            continue;
        };
        let different = previous.len() != luma.len()
            || luma
                .iter()
                .zip(previous)
                .filter(|(a, b)| a.abs_diff(**b) > 8)
                .count() as f64
                / luma.len().max(1) as f64
                > threshold;
        if changed && different {
            met = true;
            break;
        }
        if !changed {
            if different {
                stable_since = Instant::now();
                reference = Some(luma);
            } else if stable_since.elapsed() >= stable {
                met = true;
                break;
            }
        }
    }
    let jpeg = last.ok_or_else(|| ApiError::new(503, "no_signal"))?;
    permit.check()?;
    // Return the full JPEG as captured; the region only filters the comparison.
    Ok(Output {
        structured: json!({"met":met,"waited_ms":started.elapsed().as_millis() as u64,
        "region":region_for_result(region.as_ref())}),
        content: vec![data_image(&jpeg)],
    })
}

async fn history(app: &App, args: &Value) -> Result<Output, ApiError> {
    let max_frames = args["max_frames"].as_u64().unwrap_or(8) as usize;
    let window = Duration::from_secs(args["seconds"].as_u64().unwrap_or(30));
    let selected = {
        let frames = app.mcp.history.0.lock().unwrap();
        let recent: Vec<_> = frames
            .iter()
            .filter(|(time, _, _)| time.elapsed() <= window)
            .collect();
        let total = recent.len();
        if total == 0 {
            return Err(ApiError::new(503, "history_inactive"));
        }
        let count = total.min(max_frames);
        (0..count)
            .map(|i| {
                let n = if count == 1 {
                    total - 1
                } else {
                    i * (total - 1) / (count - 1)
                };
                (recent[n].1, recent[n].2.clone())
            })
            .collect::<Vec<_>>()
    };
    let mut output = Output::new(json!({"frames":[]}));
    for (ts, jpeg) in selected {
        let frame = rendered(jpeg, None, 640, None).await?;
        let mut meta = frame.structured;
        meta["ts"] = ts.into();
        output.structured["frames"]
            .as_array_mut()
            .unwrap()
            .push(meta);
        output.content.extend(frame.content);
    }
    Ok(output)
}

pub(super) async fn status(app: &App) -> Result<Value, ApiError> {
    let sampler = app.clone();
    let (power, video, keyboard, buttons, connected) = tokio::task::spawn_blocking(move || {
        (
            power_state(&sampler),
            video::hdmi(&sampler),
            sampler
                .hw
                .locks()
                .unwrap_or_else(|_| json!({"num":null,"caps":null,"scroll":null})),
            sampler.hw.power_buttons(),
            usb_connected(&sampler),
        )
    })
    .await
    .map_err(io::Error::other)?;
    let state = app.mcp.runtime.lock().unwrap();
    let usb = app.applied_usb();
    Ok(json!({
        "power":{"state":power,"power_button":buttons.0,"reset_button":buttons.1},
        "video":video,"keyboard":keyboard,
        "usb":{"connected":connected,"functions":usb,"relative_only":usb["mouse_abs"]!=true},
        "telemetry":{"online":app.mcp.metrics.online(app)},
        "host_agent":app.mcp.host.status(app),"media":app.storage.mcp_media_status(),
        "control":{"writable_sessions":app.console.writable_count(),"last_human_input_s":state.last_human.map(|t|t.elapsed().as_secs()),"stopped":state.stopped}
    }))
}
pub(super) fn power_state(app: &App) -> &'static str {
    match crate::console::panel_pc_power(app)
        .ok()
        .and_then(|v| v["on"].as_bool())
    {
        Some(true) => "on",
        Some(false) => "off",
        None => "unknown",
    }
}
pub(super) fn usb_connected(app: &App) -> bool {
    if app.sim {
        return app
            .console
            .sim_pc_connected
            .load(std::sync::atomic::Ordering::Relaxed);
    }
    // An unreadable or suspended UDC can still be attached during a write.
    let root = FsPath::new("/sys/class/udc");
    let Some(name) = fs::read_to_string("/sys/kernel/config/usb_gadget/kvmpanel/UDC").ok() else {
        return false;
    };
    if name.trim().is_empty() {
        return false;
    }
    fs::read_to_string(root.join(name.trim()).join("state")).map_or(true, |s| {
        !matches!(s.trim(), "not attached" | "powered" | "default")
    })
}

pub(super) async fn call(
    app: &App,
    name: &str,
    args: &Value,
    permit: &Permit,
) -> Result<Output, ApiError> {
    let structured = match name {
        "screenshot" => return screenshot(app, args).await,
        "screen_wait" => return wait_for_frame(app, args, permit).await,
        "screen_history" => return history(app, args).await,
        "host_status" => status(app).await?,
        "telemetry_read" => {
            if !app.mcp.metrics.online(app) {
                return Err(ApiError::new(503, "telemetry_offline"));
            }
            json!({"metrics":current(app,args)})
        }
        "telemetry_catalogue" => app.mcp.metrics.catalogue(app),
        "telemetry_watch" => watch_metrics(app, args, permit).await?,
        "host_info" => app.mcp.host.call(app, "info", permit).await?,
        "host_clipboard" => app.mcp.host.call(app, "clipboard", permit).await?,
        _ => return Err(ApiError::new(404, "not_found")),
    };
    Ok(Output::new(structured))
}

pub(super) async fn resource(app: &App, uri: &str) -> Result<Value, ApiError> {
    let contents = match uri {
        "kvmpanel://host/status" => {
            json!({"uri":uri,"mimeType":"application/json","text":status(app).await?.to_string()})
        }
        "kvmpanel://telemetry/catalogue" => {
            json!({"uri":uri,"mimeType":"application/json","text":app.mcp.metrics.catalogue(app).to_string()})
        }
        "kvmpanel://screen/latest" => {
            json!({"uri":uri,"mimeType":"image/jpeg","blob":STANDARD.encode(crate::console::snapshot(app).await?)})
        }
        _ => return Err(ApiError::new(404, "not_found")),
    };
    Ok(json!({"contents":[contents]}))
}

struct Led {
    node: PathBuf,
    old_color: String,
    old_brightness: String,
}
impl Drop for Led {
    fn drop(&mut self) {
        let _ = fs::write(self.node.join("multi_intensity"), &self.old_color);
        let _ = fs::write(self.node.join("brightness"), &self.old_brightness);
    }
}
fn leds() -> Vec<Led> {
    (1..=4)
        .filter_map(|n| {
            let node = PathBuf::from(format!("/sys/class/leds/rgb:indicator-{n}"));
            let old_color = fs::read_to_string(node.join("multi_intensity")).ok()?;
            let old_brightness = fs::read_to_string(node.join("brightness")).ok()?;
            fs::write(node.join("multi_intensity"), "255 104 0").ok()?;
            Some(Led {
                node,
                old_color,
                old_brightness,
            })
        })
        .collect()
}

pub(super) async fn background(app: &App) {
    let mut sub = None;
    let mut indicator: Option<Vec<Led>> = None;
    let mut phase = 0;
    let mut tick = tokio::time::interval(Duration::from_millis(200));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut old_activity = false;
    let mut last_frame = Instant::now();
    let mut tailscale_tick = 0;
    let mut flush_used = Instant::now();
    loop {
        tick.tick().await;
        let timestamp = crate::console::now_ms();
        if phase % 5 == 0 {
            let _ = app.mcp.audit.lock().unwrap().flush(timestamp / 60_000);
        }
        if flush_used.elapsed() >= Duration::from_secs(60) {
            flush_used = Instant::now();
            let changed = {
                let mut state = app.mcp.runtime.lock().unwrap();
                let dirty = std::mem::take(&mut state.used_dirty);
                dirty.then(|| state.used.clone())
            };
            if let Some(used) = changed {
                let update = app
                    .system
                    .change(move |s| {
                        for key in &mut s.mcp_keys {
                            if let Some(ts) = used.get(&key.id) {
                                key.last_used = Some(*ts);
                            }
                        }
                        Ok(())
                    })
                    .await;
                if update.is_err() {
                    app.mcp.runtime.lock().unwrap().used_dirty = true;
                }
            }
        }
        if let Ok(_guard) = app.mcp.input.try_lock() {
            let raw = app.mcp.raw_input.lock().unwrap().clone();
            if raw.is_some_and(|(when, id)| {
                when.elapsed() >= ACTIVITY
                    || app.config()["kvm"]["enabled"] != true
                    || !app
                        .system
                        .snapshot()
                        .mcp_keys
                        .iter()
                        .any(|key| key.id == id && key.enabled("input"))
            }) {
                operations::release_raw(app).await;
            }
        }
        let active = app.mcp.activity()["active"] == true;
        if active != old_activity {
            old_activity = active;
            app.emit("status_changed");
        }
        if active && !app.sim {
            for led in indicator.get_or_insert_with(leds) {
                let level = 4 + (phase as i32 - 8).abs() * 5;
                let _ = fs::write(led.node.join("brightness"), level.to_string());
            }
        } else {
            indicator = None;
        }
        phase = (phase + 1) % 16;
        if !app.sim && tailscale_tick == 0 {
            let app = app.clone();
            tokio::spawn(async move {
                let path = app.data_dir.join("tailscale/tailscale");
                app.mcp.tailscale.lock().unwrap().clear();
                if !path.is_file() {
                    return;
                }
                let mut cmd = tokio::process::Command::new(path);
                cmd.kill_on_drop(true)
                    .arg("--socket")
                    .arg(app.data_dir.join("tailscale/run.sock"))
                    .args(["status", "--json"]);
                let Ok(Ok(output)) =
                    tokio::time::timeout(Duration::from_secs(2), cmd.output()).await
                else {
                    return;
                };
                let Ok(status) = serde_json::from_slice::<Value>(&output.stdout) else {
                    return;
                };
                if status["BackendState"] != "Running" {
                    return;
                }
                let mut addrs = status["TailscaleIPs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                if let Some(dns) = status["Self"]["DNSName"].as_str() {
                    let dns = dns.trim_end_matches('.').to_ascii_lowercase();
                    addrs.extend(dns.split('.').next().map(str::to_owned));
                    addrs.push(dns);
                }
                *app.mcp.tailscale.lock().unwrap() = addrs;
            });
        }
        tailscale_tick = (tailscale_tick + 1) % 25;
        let history_active = app
            .mcp
            .runtime
            .lock()
            .unwrap()
            .history_access
            .is_some_and(|at| at.elapsed() < HISTORY_TIME)
            && app.system.snapshot().mcp_enabled
            && app.config()["kvm"]["enabled"] == true;
        if history_active {
            if sub.is_none() {
                sub = Some(video::subscribe(app, false).await);
            }
            if last_frame.elapsed() >= Duration::from_secs(1) {
                let Some(frames) = &mut sub else {
                    continue;
                };
                if let Frame::Jpeg(frame) = frames.rx.borrow_and_update().clone() {
                    last_frame = Instant::now();
                    let mut history = app.mcp.history.0.lock().unwrap();
                    history.push_back((last_frame, timestamp, frame));
                    let mut bytes = history.iter().map(|(_, _, data)| data.len()).sum::<usize>();
                    while history.len() > 30 || bytes > HISTORY_BYTES {
                        if let Some((_, _, old)) = history.pop_front() {
                            bytes -= old.len();
                        } else {
                            break;
                        }
                    }
                }
            }
        } else if sub.take().is_some() {
            app.mcp.history.0.lock().unwrap().clear();
        }
    }
}
