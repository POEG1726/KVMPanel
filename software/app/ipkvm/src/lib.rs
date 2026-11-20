mod auth;
mod config;
pub mod console;
pub mod hardware;
pub mod panel;
mod schema;
pub mod storage;
pub mod system;
use axum::{
    extract::{FromRequest, Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
pub use config::usb_budget;
pub use schema::validate;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::{self, Write},
    ops::Deref,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::sync::broadcast;
use tower_http::services::ServeDir;

#[derive(Debug)]
pub struct ApiError {
    pub status: u16,
    pub code: String,
    pub detail: Option<Value>,
}
impl ApiError {
    pub fn new(status: u16, code: &str) -> Self {
        Self {
            status,
            code: code.into(),
            detail: None,
        }
    }
    pub fn with_detail(mut self, v: Value) -> Self {
        self.detail = Some(v);
        self
    }
}
impl From<io::Error> for ApiError {
    fn from(e: io::Error) -> Self {
        eprintln!("kvmd: {e}");
        Self::new(500, "internal")
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut v = json!({"error":self.code});
        if let Some(d) = self.detail {
            v["detail"] = d
        }
        (StatusCode::from_u16(self.status).unwrap(), Json(v)).into_response()
    }
}
// JSON Schema accepts 100.0 for an integer. Normalize representable integral
// values before typed handlers use as_u64/as_i64.
pub fn normalize_numbers(value: &mut Value) {
    match value {
        Value::Number(n) if n.is_f64() => {
            let f = n.as_f64().unwrap();
            if f.fract() == 0.0 {
                if (0.0..18446744073709551616.0).contains(&f) {
                    *value = Value::from(f as u64)
                } else if (-9223372036854775808.0..0.0).contains(&f) {
                    *value = Value::from(f as i64)
                }
            }
        }
        Value::Array(arr) => arr.iter_mut().for_each(normalize_numbers),
        Value::Object(obj) => obj.values_mut().for_each(normalize_numbers),
        _ => {}
    }
}
pub struct JsonBody(pub Value);
impl<S: Send + Sync> FromRequest<S> for JsonBody {
    type Rejection = ApiError;
    async fn from_request(req: Request, state: &S) -> Result<Self, ApiError> {
        Json::<Value>::from_request(req, state)
            .await
            .map(|Json(mut v)| {
                normalize_numbers(&mut v);
                Self(v)
            })
            .map_err(|e| match e.status().as_u16() {
                415 => ApiError::new(415, "unsupported_type"),
                413 => ApiError::new(413, "too_large"),
                _ => ApiError::new(400, "invalid_json"),
            })
    }
}
pub fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(data)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path)?;
    fs::File::open(parent)?.sync_all()
}
fn save<T: Serialize>(path: &Path, v: &T) -> io::Result<()> {
    atomic_write(path, &serde_json::to_vec(v)?)
}
fn load_or_create<T: Serialize + DeserializeOwned>(
    path: &Path,
    init: impl FnOnce() -> io::Result<T>,
) -> io::Result<T> {
    match fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let val = init()?;
            save(path, &val)?;
            Ok(val)
        }
        Err(e) => Err(e),
    }
}
#[derive(Clone)]
pub struct App(Arc<Inner>);
impl Deref for App {
    type Target = Inner;
    fn deref(&self) -> &Inner {
        &self.0
    }
}
pub struct Inner {
    pub storage: Arc<storage::Storage>,
    pub system: system::System,
    pub console: console::Console,
    pub sim: bool,
    pub data_dir: PathBuf,
    pub started: Instant,
    pub config_updates: broadcast::Sender<Value>,
    pub events: broadcast::Sender<String>,
    pub hw: Arc<dyn hardware::Hardware>,
    auth: Mutex<auth::Auth>,
    cfg: Mutex<config::ConfigState>,
}
impl App {
    pub fn new(data_dir: &Path, sim: bool) -> io::Result<Self> {
        fs::create_dir_all(data_dir)?;
        let hw = hardware::open(sim)?;
        Ok(Self(Arc::new(Inner {
            storage: Arc::new(storage::Storage::new(data_dir, sim)?),
            system: system::System::new(data_dir)?,
            console: console::Console::new(data_dir)?,
            sim,
            data_dir: data_dir.to_owned(),
            started: Instant::now(),
            config_updates: broadcast::channel(256).0,
            events: broadcast::channel(256).0,
            hw,
            auth: Mutex::new(auth::Auth::open(data_dir)?),
            cfg: Mutex::new(config::ConfigState::open(data_dir)?),
        })))
    }
    pub fn media_status(&self) -> Value {
        self.storage.status()
    }
    pub fn emit(&self, event: &str) {
        let _ = self.events.send(event.to_owned());
    }
    pub fn socket_code(&self, h: &HeaderMap, perm: &str, event: &str) -> Option<u16> {
        match event {
            "disconnect_sessions" => Some(4409),
            "kvm_restarting" => Some(1012),
            _ => self
                .require(h, perm)
                .err()
                .map(|e| if e.status == 403 { 4403 } else { 4401 }),
        }
    }
}
pub fn router(app: App, web: &Path) -> Router {
    Router::new()
        .route(
            "/api/health",
            get(|| async { Json(json!({"ok":true,"version":env!("CARGO_PKG_VERSION")})) }),
        )
        .merge(storage::routes())
        .merge(system::routes())
        .merge(console::routes())
        .merge(auth::routes())
        .merge(config::routes())
        .route(
            "/api/{*unknown}",
            axum::routing::any(|| async { ApiError::new(404, "not_found") }),
        )
        .fallback_service(ServeDir::new(web))
        .with_state(app)
}
