//! Device administration, local shells, updates and the bearer-authenticated MCP API.
pub mod listeners;
mod network;
mod tailscale;
mod terminal;
mod update;
use crate::{atomic_write, validate, ApiError, App, JsonBody};
use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
struct Settings {
    scripts: BTreeMap<String, String>,
    network: Value,
    update: Value,
    key_hash: Option<String>,
    key_hint: Option<String>,
    mcp_enabled: bool,
    jobs: BTreeMap<String, Value>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            scripts: Default::default(),
            network: json!({"mode":"dhcp","dns":{"mode":"dhcp","servers":[]},"https":{"enabled":false,"port":443},"http_port":80,"bind":"0.0.0.0"}),
            update: json!({"channel":"stable","server":"https://updates.example.invalid/kvmpanel"}),
            key_hash: None,
            key_hint: None,
            mcp_enabled: false,
            jobs: Default::default(),
        }
    }
}
pub struct System {
    path: PathBuf,
    settings: Mutex<Settings>,
    tail: tokio::sync::Mutex<tailscale::Service>,
    net_apply: tokio::sync::Mutex<()>,
    latest: Mutex<Option<String>>,
}
impl System {
    pub fn new(data_dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(data_dir)?;
        let path = data_dir.join("system.json");
        let mut settings: Settings = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Settings::default(),
            Err(e) => return Err(e),
        };
        let mut changed = false;
        for job in settings.jobs.values_mut() {
            if job["state"] == "running" {
                job["state"] = json!("failed");
                changed = true;
            }
        }
        if changed {
            atomic_write(&path, &serde_json::to_vec(&settings)?)?;
        }
        Ok(Self {
            path,
            settings: Mutex::new(settings),
            tail: Default::default(),
            net_apply: Default::default(),
            latest: Default::default(),
        })
    }
    fn change<T>(
        &self,
        f: impl FnOnce(&mut Settings) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        let mut settings = self.settings.lock().unwrap();
        let mut next = settings.clone();
        let result = f(&mut next)?;
        let bytes = serde_json::to_vec(&next).map_err(io::Error::other)?;
        atomic_write(&self.path, &bytes)?;
        *settings = next;
        Ok(result)
    }
    pub fn network(&self) -> Value {
        self.settings.lock().unwrap().network.clone()
    }
}

pub fn routes() -> Router<App> {
    Router::new()
        .merge(terminal::routes())
        .merge(update::routes())
        .route("/api/device", get(device))
        .route("/api/device/action", post(action))
        .route("/api/network", get(network::get).put(network::put))
        .route("/api/tailscale", get(tailscale::get))
        .route("/api/tailscale/{action}", post(tailscale::action))
        .route("/api/mcp", get(mcp_state).put(set_mcp))
        .route("/api/mcp/key", delete(revoke))
        .route("/api/mcp/regenerate", post(regenerate))
        .route("/api/mcp/v1/screenshot", get(screenshot))
        .route("/api/mcp/v1/hid", post(hid))
        .route("/api/branding/logo", get(logo).put(put_logo))
        .route("/api/about", get(about))
}

fn bad_body(path: &str) -> ApiError {
    ApiError::new(400, "invalid_body").with_detail(json!({"path":path}))
}
fn missing() -> ApiError {
    ApiError::new(404, "not_found")
}
fn mime(h: &HeaderMap) -> &str {
    h.get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .unwrap_or("")
        .trim()
}
const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn device_info(app: &App) -> Value {
    let hostname = app.config()["device"]["hostname"].clone();
    if app.sim {
        return json!({"hostname":hostname,"version":VERSION,"kernel":"Linux 6.18.64",
        "uptime_s":273600+app.started.elapsed().as_secs(),"ip":"192.168.1.50","mac":"02:1a:7c:3e:51:9d","link_mbps":1000,
        "soc_temp_c":48,"mem":{"used":138000000,"total":512000000},"data":{"used":1200000000,"total":3400000000u64},"lux":212});
    }
    let read = |path: &str| {
        fs::read_to_string(path)
            .unwrap_or_default()
            .trim()
            .to_owned()
    };
    let mem = read("/proc/meminfo");
    let mem_bytes = |key: &str| {
        mem.lines()
            .find_map(|line| {
                line.strip_prefix(key)?
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()
            })
            .unwrap_or(0)
            * 1024
    };
    let total = mem_bytes("MemTotal:");
    let (used, space) = network::space(&app.data_dir).unwrap_or_default();
    json!({"hostname":hostname,"version":VERSION,"kernel":format!("Linux {}",read("/proc/sys/kernel/osrelease")),
        "uptime_s":read("/proc/uptime").split('.').next().and_then(|s|s.parse::<u64>().ok()).unwrap_or_default(),
        "ip":network::eth_ip(),"mac":read("/sys/class/net/eth0/address"),
        "link_mbps":read("/sys/class/net/eth0/speed").parse::<u32>().unwrap_or_default(),
        "soc_temp_c":app.hw.temperature().ok(),"lux":app.hw.lux().ok(),
        "mem":{"used":total.saturating_sub(mem_bytes("MemAvailable:")),"total":total},"data":{"used":used,"total":space}})
}
async fn device(State(app): State<App>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    app.require(&h, "device")?;
    Ok(Json(device_info(&app)))
}
pub fn device_action(app: &App, action: &str) -> Result<(), ApiError> {
    match action {
        "reset_hid" => {
            app.hw.reset_hid()?;
            app.emit("usb_reenumerated");
        }
        "disconnect_sessions" => app.emit("disconnect_sessions"),
        "restart_kvm" | "reboot" | "shutdown" => {
            if action == "restart_kvm" {
                app.emit("kvm_restarting");
            }
            if !app.sim {
                let (cmd, args): (&str, &[&str]) = match action {
                    "restart_kvm" => ("/etc/init.d/S95kvmd", &["restart"]),
                    "reboot" => ("/sbin/reboot", &[]),
                    _ => ("/sbin/poweroff", &[]),
                };
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    match tokio::process::Command::new(cmd).args(args).status().await {
                        Ok(s) if s.success() => {}
                        result => eprintln!("{cmd}: {result:?}"),
                    }
                });
            }
        }
        _ => return Err(ApiError::new(400, "bad_action")),
    }
    Ok(())
}
async fn action(
    State(app): State<App>,
    h: HeaderMap,
    JsonBody(body): JsonBody,
) -> Result<StatusCode, ApiError> {
    app.require(&h, "device")?;
    validate("deviceAction", &body)?;
    device_action(&app, body["action"].as_str().unwrap())?;
    Ok(StatusCode::ACCEPTED)
}
async fn about() -> Json<Value> {
    Json(json!({"name":"KVMPanel","version":VERSION,"licenses_url":"/licenses.txt"}))
}

fn mcp(s: &Settings) -> Value {
    json!({"enabled":s.mcp_enabled,"key_hint":s.key_hint})
}
async fn mcp_state(State(app): State<App>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    app.require(&h, "mcp")?;
    Ok(Json(mcp(&app.system.settings.lock().unwrap())))
}
async fn set_mcp(
    State(app): State<App>,
    h: HeaderMap,
    JsonBody(body): JsonBody,
) -> Result<Json<Value>, ApiError> {
    app.require(&h, "mcp")?;
    validate("putMcp", &body)?;
    app.system.change(|s| {
        s.mcp_enabled = body["enabled"] == true;
        if s.mcp_enabled && s.key_hash.is_none() {
            return Err(ApiError::new(409, "no_key"));
        }
        Ok(Json(mcp(s)))
    })
}
async fn revoke(State(app): State<App>, h: HeaderMap) -> Result<StatusCode, ApiError> {
    app.require(&h, "mcp")?;
    app.system.change(|s| {
        s.mcp_enabled = false;
        s.key_hash = None;
        s.key_hint = None;
        Ok(StatusCode::NO_CONTENT)
    })
}
async fn regenerate(State(app): State<App>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    app.require(&h, "mcp")?;
    let key = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    app.system.change(|s| {
        s.mcp_enabled = true;
        s.key_hash = Some(hex::encode(Sha256::digest(key.as_bytes())));
        s.key_hint = Some(key[key.len() - 4..].into());
        Ok(Json(json!({"key":key})))
    })
}
fn bearer(app: &App, h: &HeaderMap) -> Result<(), ApiError> {
    let denied = || ApiError::new(401, "unauthenticated");
    let key = h
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(denied)?;
    let settings = app.system.settings.lock().unwrap();
    if settings.mcp_enabled
        && settings.key_hash.as_deref() == Some(&hex::encode(Sha256::digest(key.as_bytes())))
    {
        Ok(())
    } else {
        Err(denied())
    }
}
async fn screenshot(State(app): State<App>, h: HeaderMap) -> Result<Response, ApiError> {
    bearer(&app, &h)?;
    let frame = crate::console::snapshot(&app).await?;
    Ok(([(header::CONTENT_TYPE, "image/jpeg")], frame).into_response())
}
async fn hid(
    State(app): State<App>,
    h: HeaderMap,
    JsonBody(body): JsonBody,
) -> Result<StatusCode, ApiError> {
    bearer(&app, &h)?;
    if body["t"] == "ping" {
        return Err(ApiError::new(400, "bad_message"));
    }
    crate::console::input(&app, &body).await?;
    Ok(StatusCode::NO_CONTENT)
}
const LOGO_MAX: usize = 256 * 1024;
async fn put_logo(
    State(app): State<App>,
    h: HeaderMap,
    body: Body,
) -> Result<StatusCode, ApiError> {
    app.require(&h, "device")?;
    let kind = match mime(&h) {
        "image/png" => 0,
        "image/svg+xml" => 1,
        _ => return Err(ApiError::new(415, "unsupported_type")),
    };
    // Drain rejected uploads as well, so the HTTP/1 connection remains
    // reusable after the 413 response. Never buffer more than the logo limit.
    let mut chunks = body.into_data_stream();
    let mut record = vec![kind];
    let mut oversized = false;
    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.map_err(|_| bad_body(""))?;
        if chunk.len() > LOGO_MAX - (record.len() - 1) {
            oversized = true;
        }
        if !oversized {
            record.extend_from_slice(&chunk);
        }
    }
    if oversized {
        return Err(ApiError::new(413, "too_large"));
    }
    atomic_write(&app.data_dir.join("logo"), &record)?;
    Ok(StatusCode::NO_CONTENT)
}
async fn logo(State(app): State<App>) -> Result<Response, ApiError> {
    let data = match tokio::fs::read(app.data_dir.join("logo")).await {
        Ok(d) => d,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(missing()),
        Err(e) => return Err(e.into()),
    };
    let kind = match data.first() {
        Some(0) => "image/png",
        Some(1) => "image/svg+xml",
        _ => return Err(io::Error::other("invalid logo record").into()),
    };
    Ok((
        [
            (header::CONTENT_TYPE, kind),
            (
                header::CONTENT_SECURITY_POLICY,
                "sandbox; default-src 'none'; style-src 'unsafe-inline'",
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        Body::from(axum::body::Bytes::from(data).slice(1..)),
    )
        .into_response())
}

/// Resume services owned by kvmd after the board boots.
pub fn start_services(app: App) {
    if !app.sim {
        let net = app.clone();
        tokio::spawn(async move {
            let cfg = net.system.network();
            if let Err(e) = network::apply(&net, &cfg).await {
                eprintln!("initial eth0 settings: {e}");
            }
        });
        tokio::spawn(async move {
            tailscale::restore(&app).await;
        });
    }
}
