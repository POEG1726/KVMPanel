use crate::{atomic_write, validate, ApiError, App, JsonBody};
use axum::{
    body::Bytes,
    extract::{
        ws::{CloseFrame, Message, WebSocket},
        ConnectInfo, Path, State, WebSocketUpgrade,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    io,
    net::SocketAddr,
    path::Path as FsPath,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{broadcast, watch};

mod video;

#[derive(Default)]
struct Sessions {
    entries: BTreeMap<u64, Value>,
    release_pending: bool,
}
impl Sessions {
    fn writable(&self, id: u64) -> bool {
        self.entries
            .get(&id)
            .is_some_and(|v| v["view_only"] == false)
    }
    fn update(&mut self, change: impl FnOnce(&mut BTreeMap<u64, Value>)) -> bool {
        let had_controller = self.entries.values().any(|v| v["view_only"] == false);
        change(&mut self.entries);
        let last_left = had_controller && !self.entries.values().any(|v| v["view_only"] == false);
        self.release_pending |= last_left;
        last_left
    }
}

pub struct Console {
    edid: Mutex<Vec<u8>>,
    wol: Mutex<Vec<Value>>,
    sessions: Mutex<Sessions>,
    // Order releases relative to inputs already in flight and the next controller's first key.
    hid_write: Mutex<()>,
    next_id: AtomicU64,
    frames: Mutex<Option<watch::Sender<Option<Bytes>>>>,
    pub sim_signal: AtomicBool,
    pub sim_pc_connected: AtomicBool,
    pub(crate) stats: Mutex<(u64, u64)>,
}

impl Console {
    pub fn new(data_dir: &FsPath) -> io::Result<Self> {
        let wol = match std::fs::read(data_dir.join("wol.json")) {
            Ok(raw) => serde_json::from_slice(&raw).map_err(io::Error::other)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => vec![],
            Err(e) => return Err(e),
        };
        Ok(Self {
            edid: Mutex::new(default_edid()),
            wol: Mutex::new(wol),
            sessions: Mutex::new(Sessions::default()),
            hid_write: Mutex::new(()),
            next_id: AtomicU64::default(),
            frames: Mutex::new(None),
            sim_signal: AtomicBool::new(true),
            sim_pc_connected: AtomicBool::new(true),
            stats: Mutex::new((0, 0)),
        })
    }
    pub fn session_count(&self) -> usize {
        self.sessions.lock().unwrap().entries.len()
    }
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub fn routes() -> Router<App> {
    Router::new()
        .route("/api/ws", get(ws))
        .route("/api/stream", get(video::stream))
        .route("/api/stream/snapshot.jpg", get(jpeg))
        .route("/api/video/edid", get(get_edid).put(put_edid))
        .route("/api/video/edid/reset", post(reset_edid))
        .route("/api/power", get(power))
        .route("/api/power/press", post(press))
        .route("/api/wol/devices", get(devices).post(add_device))
        .route("/api/wol/devices/{mac}", delete(delete_device))
        .route("/api/wol/wake", post(wake))
}

pub use video::snapshot;

async fn jpeg(State(app): State<App>, h: HeaderMap) -> Result<impl IntoResponse, ApiError> {
    app.require(&h, "console")?;
    Ok(([("content-type", "image/jpeg")], snapshot(&app).await?))
}

fn default_edid() -> Vec<u8> {
    let mut b = vec![0; 128];
    b[..25].copy_from_slice(&[
        0, 255, 255, 255, 255, 255, 255, 0, 0x2c, 0x56, 1, 0, 1, 0, 0, 0, 46, 36, 1, 3, 0x80, 0x35,
        0x1e, 0x78, 0x0a,
    ]);
    b[38..54].fill(1);
    b[54..72].copy_from_slice(&[
        0x02, 0x3a, 0x80, 0x18, 0x71, 0x38, 0x2d, 0x40, 0x58, 0x2c, 0x45, 0, 0x13, 0x2b, 0x21, 0,
        0, 0x1e,
    ]);
    b[75] = 0xfc;
    b[77..90].copy_from_slice(b"KVMPanel\n    ");
    b[93] = 0x10;
    b[111] = 0x10;
    b[127] = 0u8.wrapping_sub(b[..127].iter().copied().fold(0, u8::wrapping_add));
    b
}
fn edid_json(bytes: &[u8]) -> Value {
    json!({"hex":hex::encode(bytes),"preset":if bytes==default_edid() { "1080p" } else { "custom" }})
}
async fn get_edid(State(app): State<App>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    app.require(&h, "console")?;
    if app.sim {
        return Ok(Json(edid_json(&app.console.edid.lock().unwrap())));
    }
    let bytes = tokio::task::spawn_blocking(move || app.hw.read_edid())
        .await
        .map_err(io::Error::other)??;
    Ok(Json(edid_json(&bytes)))
}
async fn put_edid(
    State(app): State<App>,
    h: HeaderMap,
    JsonBody(v): JsonBody,
) -> Result<Json<Value>, ApiError> {
    app.require(&h, "device")?;
    validate("putEdid", &v)?;
    let data =
        hex::decode(v["hex"].as_str().unwrap()).map_err(|_| ApiError::new(400, "bad_edid"))?;
    if !matches!(data.len(), 128 | 256) || !data.starts_with(&[0, 255, 255, 255, 255, 255, 255, 0])
    {
        return Err(ApiError::new(400, "bad_edid"));
    }
    set_edid(app, data).await
}
async fn reset_edid(State(app): State<App>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    app.require(&h, "device")?;
    set_edid(app, default_edid()).await
}
async fn set_edid(app: App, bytes: Vec<u8>) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let mut edid = app.console.edid.lock().unwrap();
        app.hw.edid(&bytes)?;
        *edid = bytes;
        Ok(Json(edid_json(&edid)))
    })
    .await
    .map_err(io::Error::other)?
}

async fn power(State(app): State<App>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    app.require(&h, "console")?;
    Ok(Json(
        tokio::task::spawn_blocking(move || app.hw.power())
            .await
            .map_err(io::Error::other)??,
    ))
}
async fn press(
    State(app): State<App>,
    h: HeaderMap,
    JsonBody(v): JsonBody,
) -> Result<StatusCode, ApiError> {
    app.require(&h, "power")?;
    validate("pressPower", &v)?;
    tokio::task::spawn_blocking(move || {
        app.hw.press(
            v["button"].as_str().unwrap(),
            v["duration_ms"].as_u64().unwrap(),
        )?;
        app.emit("status_changed");
        Ok::<_, io::Error>(())
    })
    .await
    .map_err(io::Error::other)??;
    Ok(StatusCode::NO_CONTENT)
}
fn mac(text: &str) -> Option<[u8; 6]> {
    text.trim()
        .split(['-', ':'])
        .map(|s| {
            if s.len() != 2 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            u8::from_str_radix(s, 16).ok()
        })
        .collect::<Option<Vec<_>>>()?
        .try_into()
        .ok()
}
fn mac_string(bytes: &[u8; 6]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}
async fn devices(State(app): State<App>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    app.require(&h, "power")?;
    Ok(Json(json!({"devices":*app.console.wol.lock().unwrap()})))
}
fn update_wol(
    app: &App,
    change: impl FnOnce(&mut Vec<Value>) -> Result<(), ApiError>,
) -> Result<(), ApiError> {
    let mut guard = app.console.wol.lock().unwrap();
    let mut copy = guard.clone();
    change(&mut copy)?;
    atomic_write(
        &app.data_dir.join("wol.json"),
        &serde_json::to_vec(&copy).unwrap(),
    )?;
    *guard = copy;
    Ok(())
}
async fn add_device(
    State(app): State<App>,
    h: HeaderMap,
    JsonBody(v): JsonBody,
) -> Result<impl IntoResponse, ApiError> {
    app.require(&h, "power")?;
    validate("addWolDevice", &v)?;
    let mac =
        mac_string(&mac(v["mac"].as_str().unwrap()).ok_or_else(|| ApiError::new(400, "bad_mac"))?);
    let item = json!({"name":v["name"],"mac":mac});
    update_wol(&app, |items| {
        if items.iter().any(|d| d["mac"] == mac) {
            return Err(ApiError::new(409, "device_exists"));
        }
        items.push(item.clone());
        Ok(())
    })?;
    Ok((StatusCode::CREATED, Json(item)))
}
async fn delete_device(
    State(app): State<App>,
    h: HeaderMap,
    Path(raw): Path<String>,
) -> Result<StatusCode, ApiError> {
    app.require(&h, "power")?;
    let mac = mac_string(&mac(&raw).ok_or_else(|| ApiError::new(404, "not_found"))?);
    update_wol(&app, |items| {
        let index = items
            .iter()
            .position(|d| d["mac"] == mac)
            .ok_or_else(|| ApiError::new(404, "not_found"))?;
        items.remove(index);
        Ok(())
    })?;
    Ok(StatusCode::NO_CONTENT)
}
async fn wake(
    State(app): State<App>,
    h: HeaderMap,
    JsonBody(v): JsonBody,
) -> Result<StatusCode, ApiError> {
    app.require(&h, "power")?;
    validate("wakeOnLan", &v)?;
    let mac = mac(v["mac"].as_str().unwrap()).ok_or_else(|| ApiError::new(400, "bad_mac"))?;
    if !app.sim {
        let mut packet = [0xff; 102];
        for field in packet[6..].chunks_exact_mut(6) {
            field.copy_from_slice(&mac);
        }
        let sock = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
        sock.set_broadcast(true)?;
        sock.send_to(&packet, "255.255.255.255:9").await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Split a scaled scroll into signed HID deltas without wrapping or losing distance.
fn wheel_steps(dy: i64, config: &Value) -> Vec<i64> {
    let speed = config["wheel_speed"].as_i64().unwrap_or(1).clamp(1, 10);
    let direction = if config["wheel_dir"] == "reverse" {
        -1
    } else {
        1
    };
    let mut total = dy * speed * direction;
    let mut reports = Vec::new();
    while total != 0 {
        let step = total.clamp(-127, 127);
        reports.push(step);
        total -= step;
    }
    reports
}

async fn type_paste<F, Fut>(
    strokes: Vec<(u8, u8)>,
    allowed: impl Fn() -> bool,
    mut write: F,
) -> Result<(), ApiError>
where
    F: FnMut(u8, u8) -> Fut,
    Fut: std::future::Future<Output = Result<(), ApiError>>,
{
    for (modifiers, usage) in strokes {
        if !allowed() {
            return Err(ApiError::new(403, "forbidden"));
        }
        write(modifiers, usage).await?;
    }
    Ok(())
}

fn hid_write(
    app: &App,
    session: Option<u64>,
    write: impl FnOnce(&dyn crate::hardware::Hardware) -> io::Result<()>,
) -> Result<(), ApiError> {
    app.console
        .write_hid(session, || app.hw.reset_hid(), || write(app.hw.as_ref()))
}

impl Console {
    fn write_hid(
        &self,
        session: Option<u64>,
        release_keys: impl FnOnce() -> io::Result<()>,
        write: impl FnOnce() -> io::Result<()>,
    ) -> Result<(), ApiError> {
        let _guard = self.hid_write.lock().unwrap();
        let (release, permitted) = {
            let mut sessions = self.sessions.lock().unwrap();
            (
                std::mem::take(&mut sessions.release_pending),
                session.is_none_or(|id| sessions.writable(id)),
            )
        };
        if release {
            if let Err(error) = release_keys() {
                self.sessions.lock().unwrap().release_pending = true;
                return Err(error.into());
            }
        }
        if !permitted {
            return Err(ApiError::new(403, "forbidden"));
        }
        write()?;
        Ok(())
    }
}

fn update_sessions(app: &App, change: impl FnOnce(&mut BTreeMap<u64, Value>)) {
    let last_left = app.console.sessions.lock().unwrap().update(change);
    if last_left {
        let app = app.clone();
        tokio::task::spawn_blocking(move || {
            let _ = hid_write(&app, None, |_| Ok(()));
        });
    }
}

pub async fn input(app: &App, msg: &Value, allowed: impl Fn() -> bool) -> Result<(), ApiError> {
    input_for(app, msg, allowed, None).await
}
async fn input_for(
    app: &App,
    msg: &Value,
    allowed: impl Fn() -> bool,
    session: Option<u64>,
) -> Result<(), ApiError> {
    if validate("ClientMessage", msg).is_err()
        || msg["t"] == "ping"
        || msg["text"].as_str().is_some_and(|t| t.len() > 65536)
    {
        return Err(ApiError::new(400, "bad_message"));
    }
    if !allowed() {
        return Err(ApiError::new(403, "forbidden"));
    }
    let cfg = app.config();
    let hid = &cfg["hid"];
    let mut event = msg.clone();
    crate::normalize_numbers(&mut event);
    match event["t"].as_str() {
        Some("paste") if !app.sim => {
            let hw = app.hw.clone();
            let caps = tokio::task::spawn_blocking(move || hw.locks())
                .await
                .map_err(io::Error::other)??;
            let keys = crate::hardware::layout::strokes(
                hid["keyboard_layout"].as_str().unwrap_or("us"),
                event["text"].as_str().unwrap(),
                caps["caps"] == true,
            )
            .map_err(|e| {
                ApiError::new(400, "bad_message").with_detail(json!({"reason":e.to_string()}))
            })?;
            type_paste(keys, allowed, |modifiers, usage| {
                let app = app.clone();
                async move {
                    tokio::task::spawn_blocking(move || {
                        hid_write(&app, session, |hw| hw.type_key(modifiers, usage))
                    })
                    .await
                    .map_err(io::Error::other)??;
                    Ok(())
                }
            })
            .await?;
        }
        Some("wheel") => {
            for step in wheel_steps(event["dy"].as_i64().unwrap(), hid) {
                if !allowed() {
                    return Err(ApiError::new(403, "forbidden"));
                }
                let app = app.clone();
                tokio::task::spawn_blocking(move || {
                    hid_write(&app, session, |hw| hw.hid(&json!({"t":"wheel","dy":step})))
                })
                .await
                .map_err(io::Error::other)??;
            }
        }
        _ => {
            let target = app.clone();
            tokio::task::spawn_blocking(move || hid_write(&target, session, |hw| hw.hid(&event)))
                .await
                .map_err(io::Error::other)??;
            if msg["t"] == "key" || msg["t"] == "shortcut" {
                app.emit("status_changed");
            }
        }
    }
    Ok(())
}

pub async fn jiggler(app: App) {
    run_jiggler(
        || app.config()["hid"]["jiggler"] == true,
        || async {
            let hw = app.hw.clone();
            if let Ok(Err(e)) = tokio::task::spawn_blocking(move || hw.jiggle()).await {
                eprintln!("mouse jiggler: {e}");
            }
        },
    )
    .await;
}
async fn run_jiggler<Fut: std::future::Future<Output = ()>>(
    enabled: impl Fn() -> bool,
    jiggle: impl Fn() -> Fut,
) {
    let period = Duration::from_secs(30);
    let mut ticks = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        ticks.tick().await;
        if enabled() {
            jiggle().await;
        }
    }
}

async fn ws(
    State(app): State<App>,
    ws: WebSocketUpgrade,
    h: HeaderMap,
    peer: Option<axum::Extension<ConnectInfo<SocketAddr>>>,
) -> impl IntoResponse {
    let ip = peer
        .map(|p| p.0 .0.ip().to_string())
        .unwrap_or_else(|| "127.0.0.1".into());
    let events = app.events.subscribe();
    // Even the largest legal paste fits when every ASCII byte is a six-byte JSON escape.
    ws.max_message_size(512 * 1024)
        .on_upgrade(move |socket| console(socket, app, h, ip, events))
}

async fn send_json(sock: &mut WebSocket, msg: Value) -> bool {
    sock.send(Message::Text(msg.to_string().into()))
        .await
        .is_ok()
}
async fn close(sock: &mut WebSocket, code: u16) {
    let reason = match code {
        4401 => "unauthenticated",
        4403 => "forbidden",
        4409 => "disconnected by admin",
        1012 => "kvm restarting",
        _ => "closed",
    };
    let _ = sock
        .send(Message::Close(Some(CloseFrame {
            code,
            reason: reason.into(),
        })))
        .await;
}
struct Connected(App, u64);
impl Drop for Connected {
    fn drop(&mut self) {
        update_sessions(&self.0, |sessions| {
            sessions.remove(&self.1);
        });
        self.0.emit("console_sessions");
    }
}
async fn status(app: &App) -> Value {
    let app = app.clone();
    tokio::task::spawn_blocking(move || {
        let hdmi=video::hdmi(&app);
        let power=app.hw.power().unwrap_or_else(|_|json!({"on":false,"hdd":false}));
        let locks=app.hw.locks().unwrap_or_else(|_|json!({"num":false,"caps":false,"scroll":false}));
        let (fps,kbps)=if hdmi["signal"]!=true {(0,0)} else if app.sim {(28.min(app.config()["video"]["fps_max"].as_u64().unwrap_or(30)),24800)} else {*app.console.stats.lock().unwrap()};
        let sessions:Vec<_>=app.console.sessions.lock().unwrap().entries.values().cloned().collect();
        json!({"t":"status","hdmi":hdmi,"stream":{"fps":fps,"kbps":kbps},"power":power,"locks":locks,"sessions":sessions,"media":app.media_status()})
    }).await.expect("read console status")
}
async fn console(
    mut sock: WebSocket,
    app: App,
    h: HeaderMap,
    ip: String,
    mut events: broadcast::Receiver<String>,
) {
    if let Some(code) = app.socket_code(&h, "console", "") {
        close(&mut sock, code).await;
        return;
    }
    let Ok(account) = app.require(&h, "console") else {
        close(&mut sock, 4401).await;
        return;
    };
    let id = app.console.next_id.fetch_add(1, Ordering::Relaxed);
    app.console.sessions.lock().unwrap().entries.insert(
        id,
        json!({"user":account["name"],"ip":ip,"view_only":account["view_only"],"since":now_ms()}),
    );
    let _connected = Connected(app.clone(), id);
    app.emit("console_sessions");
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    if !send_json(&mut sock, status(&app).await).await {
        return;
    }
    let mut operation: Option<futures_util::future::BoxFuture<'static, Result<(), ApiError>>> =
        None;
    let mut queue = VecDeque::<Message>::new();
    loop {
        let busy = operation.is_some();
        let response = tokio::select! {
            biased;
            event=events.recv()=>{
                let event=match event{
                    Ok(s)=>s,
                    Err(broadcast::error::RecvError::Lagged(_))=>{close(&mut sock,4409).await;return;},
                    Err(_)=>return
                };
                if let Some(code)=app.socket_code(&h,"console",&event) {
                    if code==1012 {send_json(&mut sock,json!({"t":"kvm_restarting"})).await;}
                    close(&mut sock,code).await;return;
                }
                if event=="sessions_changed" {
                    if let Ok(user)=app.require(&h,"console"){
                        update_sessions(&app,|entries|{if let Some(session)=entries.get_mut(&id){session["view_only"]=user["view_only"].clone();}});
                        if user["view_only"]==true && busy{
                            operation=None;
                            send_json(&mut sock,json!({"t":"error","error":"forbidden"})).await;
                        }
                    }
                }
                match event.as_str(){
                    "usb_reenumerated"=>Some(json!({"t":event})),
                    "status_changed"|"console_sessions"|"config_changed"|"sessions_changed"=>Some(status(&app).await),
                    _=>None,
                }
            },
            _=interval.tick()=>Some(status(&app).await),
            done=async { operation.as_mut().unwrap().await }, if busy=>{
                operation=None;
                done.err().map(|e|json!({"t":"error","error":if e.code=="forbidden"{"forbidden"}else{"bad_message"}}))
            },
            msg=async {
                if !busy {
                    if let Some(m)=queue.pop_front(){return Some(Ok(m));}
                }
                sock.recv().await
            }, if !busy || queue.len()<8=>{

                let Some(Ok(m))=msg else{return};
                if let Some(code)=app.socket_code(&h,"console",""){ close(&mut sock,code).await;return; }
                if busy {
                    match m{
                        Message::Close(_)=>return,
                        Message::Ping(_)|Message::Pong(_)=>{},
                        _=>queue.push_back(m),
                    }
                    continue;
                }
                let data=match m{
                    Message::Text(text)=>serde_json::from_str::<Value>(text.as_str()).ok(),
                    Message::Close(_)=>return,
                    Message::Pong(_)|Message::Ping(_)=>continue,
                    Message::Binary(_)=>None,
                };
                match data{
                    Some(v) if validate("ClientMessage",&v).is_ok()=>{
                        if v["t"]=="ping"{Some(json!({"t":"pong"}))}
                        else if app.require(&h,"console").map_or(true,|u|u["view_only"]==true) {Some(json!({"t":"error","error":"forbidden"}))}
                        else {
                            let app=app.clone();
                            let h=h.clone();
                            operation=Some(Box::pin(async move{
                                input_for(&app,&v,||app.require(&h,"console").is_ok_and(|u|u["view_only"]!=true),Some(id)).await
                            }));
                            None
                        }
                    }
                    _=>Some(json!({"t":"error","error":"bad_message"}))
                }
            }
        };
        if let Some(msg) = response {
            if !send_json(&mut sock, msg).await {
                return;
            }
        }
    }
}

#[cfg(test)]
mod input_tests {
    use super::*;
    #[test]
    fn only_the_last_input_session_leaves_a_pending_release() {
        use std::cell::RefCell;
        let dir = tempfile::tempdir().unwrap();
        let console = Console::new(dir.path()).unwrap();
        let log = RefCell::new(Vec::new());
        let record = |effect| {
            log.borrow_mut().push(effect);
            Ok(())
        };
        let control = json!({"view_only":false});
        let observer = json!({"view_only":true});
        let mut sessions = console.sessions.lock().unwrap();
        sessions.entries.insert(1, control.clone());
        sessions.entries.insert(2, control.clone());
        sessions.entries.insert(3, observer.clone());
        assert!(!sessions.update(|entries| {
            entries.remove(&1);
        }));
        drop(sessions);
        console
            .write_hid(Some(2), || record("reset"), || record("existing"))
            .unwrap();
        let mut sessions = console.sessions.lock().unwrap();
        assert!(sessions.update(|entries| {
            entries.insert(2, observer);
        }));
        // The successor can register and its write can acquire the hardware lock
        // before the pending idle-cleanup task. Exactly one release precedes its key.
        sessions.entries.insert(4, control);
        drop(sessions);
        console
            .write_hid(Some(4), || record("reset"), || record("new"))
            .unwrap();
        console
            .write_hid(None, || record("reset"), || Ok(()))
            .unwrap();
        let denied = console
            .write_hid(Some(2), || record("reset"), || record("stale"))
            .unwrap_err();
        assert_eq!(denied.status, 403);
        let mut sessions = console.sessions.lock().unwrap();
        assert!(!sessions.update(|entries| {
            entries.remove(&3);
        }));
        assert!(sessions.update(|entries| {
            entries.remove(&4);
        }));
        drop(sessions);
        console
            .write_hid(None, || record("reset"), || Ok(()))
            .unwrap();
        assert_eq!(*log.borrow(), ["existing", "reset", "new", "reset"]);
    }

    #[test]
    fn wheel_uses_signed_hid_range_and_preserves_total() {
        let cfg = json!({"wheel_speed":10,"wheel_dir":"reverse"});
        assert_eq!(wheel_steps(-127, &cfg), vec![127; 10]);
        assert_eq!(wheel_steps(19, &cfg), [-127, -63]);
        assert!(wheel_steps(0, &cfg).is_empty());
        assert_eq!(
            wheel_steps(42, &json!({"wheel_speed":1,"wheel_dir":"normal"})),
            [42]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn jiggler_follows_switch_without_console_clients() {
        use std::sync::Arc;
        let enabled = Arc::new(AtomicBool::new(false));
        let reports = Arc::new(AtomicU64::new(0));
        let task_enabled = enabled.clone();
        let task_reports = reports.clone();
        let task = tokio::spawn(async move {
            run_jiggler(
                || task_enabled.load(Ordering::Relaxed),
                || {
                    task_reports.fetch_add(1, Ordering::Relaxed);
                    std::future::ready(())
                },
            )
            .await;
        });
        tokio::task::yield_now().await;
        for (active, expected) in [(false, 0), (true, 1), (true, 2), (false, 2)] {
            enabled.store(active, Ordering::Relaxed);
            tokio::time::advance(Duration::from_secs(30)).await;
            tokio::task::yield_now().await;
            assert_eq!(reports.load(Ordering::Relaxed), expected);
        }
        task.abort();
    }

    #[tokio::test]
    async fn paste_stops_between_reports_after_revocation() {
        use std::cell::Cell;
        let sent = Cell::new(0);
        let error = type_paste(
            vec![(0, 4); 4096],
            || sent.get() < 3,
            |_, _| {
                sent.set(sent.get() + 1);
                std::future::ready(Ok(()))
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error.status, 403);
        assert_eq!(sent.get(), 3);
    }
}
