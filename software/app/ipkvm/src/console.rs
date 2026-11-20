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
    collections::BTreeMap,
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

pub struct Console {
    edid: Mutex<Vec<u8>>,
    wol: Mutex<Vec<Value>>,
    sessions: Mutex<BTreeMap<u64, Value>>,
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
            sessions: Mutex::new(BTreeMap::new()),
            next_id: AtomicU64::default(),
            frames: Mutex::new(None),
            sim_signal: AtomicBool::new(true),
            sim_pc_connected: AtomicBool::new(true),
            stats: Mutex::new((0, 0)),
        })
    }
    pub fn session_count(&self) -> usize {
        self.sessions.lock().unwrap().len()
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
    Ok(Json(edid_json(&app.console.edid.lock().unwrap())))
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

pub async fn input(app: &App, msg: &Value) -> Result<(), ApiError> {
    if validate("ClientMessage", msg).is_err()
        || msg["t"] == "ping"
        || msg["text"].as_str().is_some_and(|text| text.len() > 65536)
    {
        return Err(ApiError::new(400, "bad_message"));
    }
    let hw = app.hw.clone();
    let event = msg.clone();
    tokio::task::spawn_blocking(move || hw.hid(&event))
        .await
        .map_err(io::Error::other)??;
    if msg["t"] == "key" || msg["t"] == "shortcut" {
        app.emit("status_changed");
    }
    Ok(())
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
    ws.on_upgrade(move |socket| console(socket, app, h, ip, events))
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
        self.0.console.sessions.lock().unwrap().remove(&self.1);
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
        let sessions:Vec<_>=app.console.sessions.lock().unwrap().values().cloned().collect();
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
    app.console.sessions.lock().unwrap().insert(
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
    loop {
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
                match event.as_str(){
                    "usb_reenumerated"=>Some(json!({"t":event})),
                    "status_changed"|"console_sessions"|"config_changed"|"sessions_changed"=>Some(status(&app).await),
                    _=>None,
                }
            },
            _=interval.tick()=>Some(status(&app).await),
            msg=sock.recv()=>{
                let Some(Ok(m))=msg else{return};
                if let Some(code)=app.socket_code(&h,"console",""){ close(&mut sock,code).await;return; }
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
                        else if input(&app,&v).await.is_err(){Some(json!({"t":"error","error":"bad_message"}))}
                        else {None}
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
