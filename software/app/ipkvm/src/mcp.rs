use crate::{validate, ApiError, App, JsonBody};
use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{header, HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path as FsPath, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use tokio::sync::{watch, Mutex as AsyncMutex};
use uuid::Uuid;

mod host;
mod image;
mod observe;
mod operations;

const ALLOWED_VERSIONS: [&str; 2] = ["2025-06-18", "2025-11-25"];
const CONFIRM_LIFETIME: Duration = Duration::from_secs(30);
const ACTIVITY: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct InputPolicy {
    pub(crate) human_input_priority: bool,
    pub(crate) human_input_wait_s: u64,
}

impl Default for InputPolicy {
    fn default() -> Self {
        Self {
            human_input_priority: true,
            human_input_wait_s: 2,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Key {
    id: String,
    name: String,
    scopes: Vec<String>,
    created: u64,
    last_used: Option<u64>,
    hint: String,
    hash: String,
}

impl Key {
    pub(crate) fn migrated(hash: String, hint: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: "default".into(),
            scopes: vec!["observe".into(), "input".into()],
            created: crate::console::now_ms(),
            last_used: None,
            hint,
            hash,
        }
    }

    fn enabled(&self, scope: &str) -> bool {
        self.scopes.iter().any(|item| item == scope)
    }

    fn entry(&self, used: &BTreeMap<String, u64>) -> Value {
        json!({
            "id":self.id,"name":self.name,"scopes":self.scopes,"created":self.created,
            "last_used":used.get(&self.id).copied().or(self.last_used),"hint":self.hint,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Interrupt {
    Running,
    Stopped,
    Human,
}
impl Interrupt {
    fn check(self) -> Result<(), ApiError> {
        match self {
            Self::Running => Ok(()),
            Self::Stopped => Err(ApiError::new(409, "stopped")),
            Self::Human => Err(ApiError::new(409, "human_took_over")),
        }
    }
}

struct Permit {
    stop: watch::Receiver<Interrupt>,
    human: Option<watch::Receiver<Interrupt>>,
}

impl Permit {
    fn check(&self) -> Result<(), ApiError> {
        self.stop.borrow().check()?;
        self.human
            .as_ref()
            .map_or(Ok(()), |signal| signal.borrow().check())
    }

    fn running(&self) -> bool {
        self.check().is_ok()
    }

    async fn interrupted(&self) -> ApiError {
        let mut stop = self.stop.clone();
        let mut human = self.human.clone();
        loop {
            if let Err(error) = self.check() {
                return error;
            }
            tokio::select! {
                _ = stop.changed() => {},
                _ = async {
                    match &mut human {
                        Some(human) => { let _ = human.changed().await; }
                        None => std::future::pending().await,
                    }
                } => {},
            }
        }
    }

    async fn sleep(&self, time: Duration) -> Result<(), ApiError> {
        tokio::select! {
            biased;
            e = self.interrupted() => Err(e),
            _ = tokio::time::sleep(time) => Ok(()),
        }
    }
}

struct Confirmation {
    expires: Instant,
    key: String,
    tool: String,
    args: Value,
    state: Value,
}

struct Runtime {
    stopped: bool,
    blocked: bool,
    stop: watch::Sender<Interrupt>,
    human: watch::Sender<Interrupt>,
    last_human: Option<Instant>,
    used: BTreeMap<String, u64>,
    active: Option<(Instant, String, String)>,
    active_calls: usize,
    used_dirty: bool,
    history_access: Option<Instant>,
    tokens: BTreeMap<String, Confirmation>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            stopped: false,
            blocked: false,
            stop: watch::channel(Interrupt::Running).0,
            human: watch::channel(Interrupt::Running).0,
            last_human: None,
            used: BTreeMap::new(),
            active: None,
            active_calls: 0,
            used_dirty: false,
            history_access: None,
            tokens: BTreeMap::new(),
        }
    }
}

pub(crate) struct Mcp {
    runtime: Mutex<Runtime>,
    pub(crate) input: AsyncMutex<()>,
    media: AsyncMutex<()>,
    power: AsyncMutex<()>,
    management: AsyncMutex<()>,
    audit: Mutex<Audit>,
    pub(crate) host: host::HostAgent,
    pub(crate) metrics: observe::Telemetry,
    history: observe::History,
    pub(crate) raw_input: Mutex<Option<(Instant, String)>>,
    pub(crate) tailscale: Mutex<Vec<String>>,
}

impl Mcp {
    pub(crate) fn new(data_dir: &FsPath) -> Self {
        Self {
            runtime: Mutex::default(),
            input: AsyncMutex::default(),
            media: AsyncMutex::default(),
            power: AsyncMutex::default(),
            management: AsyncMutex::default(),
            audit: Mutex::new(Audit::new(data_dir)),
            host: host::HostAgent::default(),
            metrics: observe::Telemetry::default(),
            history: observe::History::default(),
            raw_input: Mutex::default(),
            tailscale: Mutex::default(),
        }
    }

    pub(crate) fn activity(&self) -> Value {
        let runtime = self.runtime.lock().unwrap();
        runtime
            .active
            .as_ref()
            .filter(|(at, _, _)| runtime.active_calls > 0 || at.elapsed() < ACTIVITY)
            .map_or_else(
                || json!({"active":false,"key_name":null,"tool":null}),
                |(_, key, tool)| json!({"active":true,"key_name":key,"tool":tool}),
            )
    }

    pub(crate) fn human_input(&self, app: &App) -> bool {
        let mut state = self.runtime.lock().unwrap();
        state.last_human = Some(Instant::now());
        let priority = app.system.snapshot().mcp_input_policy.human_input_priority;
        if priority {
            state.human.send_replace(Interrupt::Human);
            state.human = watch::channel(Interrupt::Running).0;
        }
        priority
    }

    fn permit(&self, app: &App, input: bool) -> Result<Permit, ApiError> {
        let state = self.runtime.lock().unwrap();
        let settings = app.system.snapshot();
        if state.blocked || !settings.mcp_enabled || app.config()["kvm"]["enabled"] != true {
            return Err(ApiError::new(403, "stopped"));
        }
        if input
            && settings.mcp_input_policy.human_input_priority
            && app.console.writable_count() > 0
            && state.last_human.is_some_and(|when| {
                when.elapsed() < Duration::from_secs(settings.mcp_input_policy.human_input_wait_s)
            })
        {
            return Err(ApiError::new(409, "human_active"));
        }
        Ok(Permit {
            stop: state.stop.subscribe(),
            human: input.then(|| state.human.subscribe()),
        })
    }

    fn touch(&self, app: &App, key: &Key, tool: Option<&str>) {
        let mut runtime = self.runtime.lock().unwrap();
        let now = Instant::now();
        runtime
            .used
            .insert(key.id.clone(), crate::console::now_ms());
        runtime.used_dirty = true;
        runtime.history_access = Some(now);
        if let Some(tool) = tool {
            runtime.active = Some((now, key.name.clone(), tool.into()));
            app.emit("mcp_activity");
            app.emit("status_changed");
        }
    }

    pub(crate) fn kvm_paused(&self) {
        let mut runtime = self.runtime.lock().unwrap();
        runtime.active = None;
        runtime.tokens.clear();
        runtime.history_access = None;
        runtime.stop.send_replace(Interrupt::Stopped);
        runtime.stop = watch::channel(Interrupt::Running).0;
    }

    fn cancel(&self, stopped: bool) {
        let mut runtime = self.runtime.lock().unwrap();
        runtime.stopped = stopped;
        runtime.blocked = true;
        runtime.active = None;
        runtime.tokens.clear();
        runtime.history_access = None;
        runtime.stop.send_replace(Interrupt::Stopped);
        runtime.stop = watch::channel(Interrupt::Running).0;
    }

    fn active_call<'a>(&'a self, app: &App, key: &Key, tool: &str) -> ActiveCall<'a> {
        self.touch(app, key, Some(tool));
        self.runtime.lock().unwrap().active_calls += 1;
        ActiveCall(self)
    }

    fn confirm(
        &self,
        key: &Key,
        tool: &str,
        args: &Value,
        state: Value,
        summary: &str,
    ) -> Result<Option<Value>, ApiError> {
        let mut runtime = self.runtime.lock().unwrap();
        runtime
            .tokens
            .retain(|_, item| item.expires > Instant::now());
        let mut canonical = args.clone();
        canonical.as_object_mut().unwrap().remove("confirm_token");
        if let Some(token) = args["confirm_token"].as_str() {
            let valid = runtime.tokens.get(token).is_some_and(|item| {
                item.key == key.id
                    && item.tool == tool
                    && item.args == canonical
                    && item.state == state
            });
            if valid {
                runtime.tokens.remove(token);
                return Ok(None);
            }
            return Err(ApiError::new(409, "invalid_confirmation"));
        }
        if runtime.tokens.len() >= 128 {
            return Err(ApiError::new(429, "busy"));
        }
        let token = Uuid::new_v4().to_string();
        runtime.tokens.insert(
            token.clone(),
            Confirmation {
                expires: Instant::now() + CONFIRM_LIFETIME,
                key: key.id.clone(),
                tool: tool.into(),
                args: canonical,
                state,
            },
        );
        Ok(Some(
            json!({"needs_confirm":true,"confirm_token":token,"expires_in":30,"summary":summary}),
        ))
    }
}

struct ActiveCall<'a>(&'a Mcp);
impl Drop for ActiveCall<'_> {
    fn drop(&mut self) {
        let mut runtime = self.0.runtime.lock().unwrap();
        runtime.active_calls -= 1;
        if let Some((at, _, _)) = &mut runtime.active {
            *at = Instant::now();
        }
    }
}

pub(crate) fn routes() -> Router<App> {
    Router::new()
        .route(
            "/api/mcp",
            get(get_settings)
                .put(put_settings)
                .patch(patch_input_policy),
        )
        .route("/api/mcp/keys", post(create_key))
        .route(
            "/api/mcp/keys/{id}",
            axum::routing::patch(edit_key).delete(delete_key),
        )
        .route("/api/mcp/stop", post(stop))
        .route("/api/mcp/audit", get(audit))
        .route(
            "/api/mcp/v1",
            post(rpc).get(|| async { StatusCode::METHOD_NOT_ALLOWED }),
        )
        .route("/api/mcp/v1/screenshot", get(rest_screenshot))
        .route("/api/mcp/v1/hid", post(rest_hid))
        .layer(DefaultBodyLimit::max(1024 * 1024))
}

fn settings(app: &App) -> Value {
    let settings = app.system.snapshot();
    let state = app.mcp.runtime.lock().unwrap();
    let entries: Vec<_> = settings
        .mcp_keys
        .iter()
        .map(|key| key.entry(&state.used))
        .collect();
    json!({"enabled":settings.mcp_enabled,"keys":entries,
        "human_input_priority":settings.mcp_input_policy.human_input_priority,
        "human_input_wait_s":settings.mcp_input_policy.human_input_wait_s})
}

async fn get_settings(State(app): State<App>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    app.require(&h, "mcp")?;
    Ok(Json(settings(&app)))
}
async fn put_settings(
    State(app): State<App>,
    h: HeaderMap,
    JsonBody(body): JsonBody,
) -> Result<Json<Value>, ApiError> {
    app.require(&h, "mcp")?;
    validate("putMcp", &body)?;
    let enabled = body["enabled"] == true;
    if !enabled {
        stop_service(&app, false).await?;
    } else {
        let _management = app.mcp.management.lock().await;
        app.system
            .change(|settings| {
                if settings.mcp_keys.is_empty() {
                    return Err(ApiError::new(409, "no_key"));
                }
                settings.mcp_enabled = true;
                Ok(())
            })
            .await?;
        let mut state = app.mcp.runtime.lock().unwrap();
        state.stopped = false;
        state.blocked = false;
    }
    Ok(Json(settings(&app)))
}

async fn patch_input_policy(
    State(app): State<App>,
    h: HeaderMap,
    JsonBody(mut body): JsonBody,
) -> Result<Json<Value>, ApiError> {
    app.require(&h, "mcp")?;
    validate("patchMcpInputPolicy", &body)?;
    crate::normalize_numbers(&mut body);
    let _management = app.mcp.management.lock().await;
    app.system
        .change(move |settings| {
            if let Some(priority) = body["human_input_priority"].as_bool() {
                settings.mcp_input_policy.human_input_priority = priority;
            }
            if let Some(seconds) = body["human_input_wait_s"].as_u64() {
                settings.mcp_input_policy.human_input_wait_s = seconds;
            }
            Ok(())
        })
        .await?;
    Ok(Json(settings(&app)))
}

fn digest(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}
fn key_fields(body: &Value) -> (String, Vec<String>) {
    (
        body["name"].as_str().unwrap().to_owned(),
        body["scopes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|scope| scope.as_str().unwrap().to_owned())
            .collect(),
    )
}
async fn create_key(
    State(app): State<App>,
    h: HeaderMap,
    JsonBody(body): JsonBody,
) -> Result<Response, ApiError> {
    app.require(&h, "mcp")?;
    validate("createMcpKey", &body)?;
    let (name, scopes) = key_fields(&body);
    let mut entropy = [0u8; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut entropy)?;
    let raw = format!("kvp_{}", URL_SAFE_NO_PAD.encode(entropy));
    let key = Key {
        id: Uuid::new_v4().to_string(),
        name,
        scopes,
        hint: raw[raw.len() - 4..].to_owned(),
        created: crate::console::now_ms(),
        last_used: None,
        hash: digest(&raw),
    };
    let entry = key.entry(&BTreeMap::new());
    let name = key.name.clone();
    app.system
        .change(move |s| {
            if s.mcp_keys.len() >= 8 {
                return Err(ApiError::new(409, "key_limit"));
            }
            s.mcp_keys.push(key);
            Ok(())
        })
        .await?;
    record(&app, Some(&name), "key_create", json!({}), "ok", 0, None);
    Ok((StatusCode::CREATED, Json(json!({"key":raw,"entry":entry}))).into_response())
}
async fn edit_key(
    State(app): State<App>,
    Path(id): Path<String>,
    h: HeaderMap,
    JsonBody(body): JsonBody,
) -> Result<Json<Value>, ApiError> {
    app.require(&h, "mcp")?;
    validate("editMcpKey", &body)?;
    let (name, scopes) = key_fields(&body);
    let key = app
        .system
        .change(move |s| {
            let key = s
                .mcp_keys
                .iter_mut()
                .find(|key| key.id == id)
                .ok_or_else(|| ApiError::new(404, "not_found"))?;
            key.name = name;
            key.scopes = scopes;
            Ok(key.clone())
        })
        .await?;
    record(&app, Some(&key.name), "key_edit", json!({}), "ok", 0, None);
    Ok(Json(key.entry(&app.mcp.runtime.lock().unwrap().used)))
}
async fn delete_key(
    State(app): State<App>,
    Path(id): Path<String>,
    h: HeaderMap,
) -> Result<StatusCode, ApiError> {
    app.require(&h, "mcp")?;
    let (key, empty) = app
        .system
        .change(move |s| {
            let i = s
                .mcp_keys
                .iter()
                .position(|key| key.id == id)
                .ok_or_else(|| ApiError::new(404, "not_found"))?;
            let key = s.mcp_keys.remove(i);
            let empty = s.mcp_keys.is_empty();
            if empty {
                s.mcp_enabled = false;
            }
            Ok((key, empty))
        })
        .await?;
    {
        let mut state = app.mcp.runtime.lock().unwrap();
        state.used.remove(&key.id);
        state.tokens.retain(|_, item| item.key != key.id);
    }
    if empty {
        stop_service(&app, false).await?;
    }
    record(
        &app,
        Some(&key.name),
        "key_delete",
        json!({}),
        "ok",
        0,
        None,
    );
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn stop_service(app: &App, emergency: bool) -> Result<(), ApiError> {
    let _management = app.mcp.management.lock().await;
    app.mcp.cancel(emergency);
    app.emit("mcp_activity");
    app.emit("status_changed");
    {
        let _input = app.mcp.input.lock().await;
        operations::release_raw(app).await;
    }
    // Any in-flight power operation owns this gate until the physical line is idle.
    let _power = app.mcp.power.lock().await;
    app.system
        .change(|s| {
            s.mcp_enabled = false;
            Ok(())
        })
        .await?;
    if emergency {
        record(app, None, "stop", json!({}), "ok", 0, None);
    }
    Ok(())
}

// The ADC function-key dispatcher can call the same action used by the panel.
pub(crate) async fn stop_from_function_key(app: &App) -> Result<(), ApiError> {
    stop_service(app, true).await
}

async fn stop(State(app): State<App>, h: HeaderMap) -> Result<StatusCode, ApiError> {
    if app.require(&h, "console")?["view_only"] == true {
        return Err(ApiError::new(403, "forbidden"));
    }
    stop_service(&app, true).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn local_addresses() -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    let mut head = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return result;
    }
    let mut next = head;
    while !next.is_null() {
        let item = unsafe { &*next };
        if !item.ifa_addr.is_null() {
            match unsafe { (*item.ifa_addr).sa_family } as i32 {
                libc::AF_INET => {
                    let addr = unsafe { &*item.ifa_addr.cast::<libc::sockaddr_in>() };
                    result.insert(
                        std::net::Ipv4Addr::from(addr.sin_addr.s_addr.to_ne_bytes()).to_string(),
                    );
                }
                libc::AF_INET6 => {
                    let addr = unsafe { &*item.ifa_addr.cast::<libc::sockaddr_in6>() };
                    result.insert(std::net::Ipv6Addr::from(addr.sin6_addr.s6_addr).to_string());
                }
                _ => {}
            }
        }
        next = item.ifa_next;
    }
    unsafe { libc::freeifaddrs(head) };
    result
}
fn bare_host(authority: &str) -> Option<String> {
    let authority = authority.parse::<axum::http::uri::Authority>().ok()?;
    if authority.as_str().contains('@') {
        return None;
    }
    Some(
        authority
            .host()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .trim_end_matches('.')
            .to_ascii_lowercase(),
    )
}
async fn authenticate(app: &App, uri: &Uri, h: &HeaderMap) -> Result<Key, ApiError> {
    if app.mcp.runtime.lock().unwrap().blocked
        || !app.system.snapshot().mcp_enabled
        || app.config()["kvm"]["enabled"] != true
    {
        return Err(ApiError::new(403, "mcp_disabled"));
    }
    let cfg = app.config();
    let name = cfg["device"]["hostname"]
        .as_str()
        .unwrap_or("kvmpanel")
        .to_ascii_lowercase();
    let mut allowed = local_addresses();
    allowed.extend([
        "127.0.0.1".into(),
        "localhost".into(),
        format!("{name}.local"),
        name,
    ]);
    let tail = crate::system::mcp_tail_address(app);
    allowed.extend(tail);
    let host = h
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .or_else(|| uri.authority().map(|a| a.as_str()))
        .and_then(bare_host);
    let valid_origin = match h.get(header::ORIGIN) {
        None => true,
        Some(origin) => origin
            .to_str()
            .ok()
            .and_then(|v| v.parse::<Uri>().ok())
            .filter(|uri| matches!(uri.scheme_str(), Some("http" | "https")))
            .filter(|uri| uri.path() == "/" && uri.query().is_none())
            .and_then(|uri| bare_host(uri.authority()?.as_str()))
            .is_some_and(|origin| allowed.contains(&origin)),
    };
    if host.is_none_or(|host| !allowed.contains(&host)) || !valid_origin {
        return Err(ApiError::new(403, "bad_origin"));
    }
    let raw = h
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError::new(401, "unauthenticated"))?;
    let digest = digest(raw);
    let settings = app.system.snapshot();
    let mut found = None;
    for key in &settings.mcp_keys {
        if bool::from(digest.as_bytes().ct_eq(key.hash.as_bytes())) {
            found = Some(key);
        }
    }
    found
        .cloned()
        .ok_or_else(|| ApiError::new(401, "unauthenticated"))
}

fn rpc_reply(id: &Value, result: Value) -> Response {
    Json(json!({"jsonrpc":"2.0","id":id,"result":result})).into_response()
}
fn rpc_error(id: &Value, code: i32, message: &str) -> Response {
    Json(json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})).into_response()
}
fn version(requested: Option<&str>) -> &'static str {
    ALLOWED_VERSIONS
        .into_iter()
        .find(|v| Some(*v) == requested)
        .unwrap_or(ALLOWED_VERSIONS[1])
}

async fn rpc(
    State(app): State<App>,
    uri: Uri,
    h: HeaderMap,
    body: Result<JsonBody, ApiError>,
) -> Result<Response, ApiError> {
    let key = authenticate(&app, &uri, &h).await?;
    let body = match body {
        Ok(JsonBody(body)) => body,
        Err(error) if error.code == "invalid_json" => {
            return Ok(rpc_error(&Value::Null, -32700, "Parse error"))
        }
        Err(error) => return Err(error),
    };
    let id = &body["id"];
    let Some(method) = body["method"].as_str().filter(|_| body["jsonrpc"] == "2.0") else {
        return Ok(rpc_error(&Value::Null, -32600, "Invalid request"));
    };
    if body.get("id").is_none() {
        return Ok(StatusCode::ACCEPTED.into_response());
    }
    if !id.is_number() && !id.is_string() {
        return Ok(rpc_error(&Value::Null, -32600, "Invalid request id"));
    }
    if method != "initialize"
        && h.get("mcp-protocol-version").is_some_and(|v| {
            v.to_str()
                .ok()
                .is_none_or(|v| !ALLOWED_VERSIONS.contains(&v))
        })
    {
        return Err(ApiError::new(400, "unsupported_protocol_version"));
    }
    app.mcp.touch(&app, &key, None);
    let p = &body["params"];
    let result = match method {
        "initialize" => json!({
            "protocolVersion":version(p["protocolVersion"].as_str().or_else(|| h.get("mcp-protocol-version").and_then(|v| v.to_str().ok()))),
            "capabilities":{"tools":{},"resources":{},"prompts":{}},
            "serverInfo":{"name":"kvmpanel","version":env!("CARGO_PKG_VERSION")},
            "instructions":"Coordinates use 0–1 over the full HDMI frame from top left. Call host_status first. Dangerous actions require a confirmation token.",
        }),
        "ping" => json!({}),
        "tools/list" => {
            let tools: Vec<_> = crate::schema::mcp_tools()
                .iter()
                .filter(|(_, v)| key.enabled(v["scope"].as_str().unwrap()))
                .map(|(name, item)| {
                    let mut out = item.clone();
                    let map = out.as_object_mut().unwrap();
                    map.remove("scope");
                    map.insert("name".into(), json!(name));
                    out
                })
                .collect();
            json!({"tools":tools})
        }
        "tools/call" => {
            let Some(name) = p["name"].as_str() else {
                return Ok(rpc_error(id, -32602, "Invalid params"));
            };
            let Some(tool) = crate::schema::mcp_tools().get(name) else {
                return Ok(rpc_error(id, -32602, "Unknown tool"));
            };
            let scope = tool["scope"].as_str().unwrap();
            if !key.enabled(scope) {
                record(
                    &app,
                    Some(&key.name),
                    name,
                    json!({}),
                    "forbidden_scope",
                    0,
                    (scope == "observe").then_some(&key.id),
                );
                return Ok(rpc_reply(
                    id,
                    tool_error(ApiError::new(403, "forbidden_scope")),
                ));
            }
            let args = p.get("arguments").cloned().unwrap_or_else(|| json!({}));
            if validate(&format!("mcp/{name}"), &args).is_err() || !operations::valid(name, &args) {
                return Ok(rpc_error(id, -32602, "Invalid tool arguments"));
            }
            let started = Instant::now();
            let mutating = match (name, args["action"].as_str()) {
                ("boot_watch", _) => args["start"].as_str().is_some_and(|s| s != "none"),
                ("virtual_media", Some("list" | "status")) | ("share", Some("status")) => false,
                _ => matches!(scope, "input" | "power" | "media"),
            };
            let _activity = mutating.then(|| app.mcp.active_call(&app, &key, name));
            let out = operations::call(&app, &key, name, &args).await;
            let result = out.as_ref().map_or_else(
                |e| e.code.as_str(),
                |out| {
                    if out.structured["needs_confirm"] == true {
                        "needs_confirm"
                    } else {
                        "ok"
                    }
                },
            );
            record(
                &app,
                Some(&key.name),
                name,
                summarize(&args),
                result,
                started.elapsed().as_millis() as u64,
                (scope == "observe").then_some(&key.id),
            );
            match out {
                Ok(out) => out.result(),
                Err(e) => tool_error(e),
            }
        }
        "resources/list" => {
            json!({"resources":if key.enabled("observe") { resources() } else { vec![] }})
        }
        "resources/read" if key.enabled("observe") => {
            match observe::resource(&app, p["uri"].as_str().unwrap_or("")).await {
                Ok(r) => r,
                Err(e) => return Ok(rpc_error(id, -32002, &e.code)),
            }
        }
        "resources/read" => return Ok(rpc_error(id, -32001, "forbidden_scope")),
        "prompts/list" => {
            json!({"prompts":PROMPTS.iter().map(|(name, description)| json!({"name":name,"description":description})).collect::<Vec<_>>()})
        }
        "prompts/get" => {
            let Some((_, text)) = PROMPTS.iter().find(|(name, _)| p["name"] == *name) else {
                return Ok(rpc_error(id, -32602, "Unknown prompt"));
            };
            json!({"messages":[{"role":"user","content":{"type":"text","text":text}}]})
        }
        _ => return Ok(rpc_error(id, -32601, "Method not found")),
    };
    Ok(rpc_reply(id, result))
}

fn resources() -> Vec<Value> {
    [
        ("kvmpanel://host/status", "Host status", "application/json"),
        (
            "kvmpanel://telemetry/catalogue",
            "Telemetry catalogue",
            "application/json",
        ),
        (
            "kvmpanel://screen/latest",
            "Latest HDMI screenshot",
            "image/jpeg",
        ),
    ]
    .map(|(uri, name, mime)| json!({"uri":uri,"name":name,"mimeType":mime}))
    .to_vec()
}

const PROMPTS: [(&str, &str); 3] = [
    ("boot_failure_triage", "Read host_status, then run boot_watch to check power, HDMI and OS milestones. Use screen_history for transient errors. Ask before confirmed power operations."),
    ("stability_check", "Read telemetry_catalogue and run telemetry_watch while the load is active. Report warning counts and missing samples separately."),
    ("enter_bios", "Check host_status, then call enter_firmware with target setup and the appropriate starting action. Inspect its screenshot before navigating. Ask before reset or a power cycle."),
];

struct Output {
    structured: Value,
    content: Vec<Value>,
}
impl Output {
    fn new(structured: Value) -> Self {
        Self {
            structured,
            content: Vec::new(),
        }
    }
    fn result(mut self) -> Value {
        self.content
            .insert(0, json!({"type":"text","text":self.structured.to_string()}));
        json!({"structuredContent":self.structured,"content":self.content,"isError":false})
    }
}
fn tool_error(error: ApiError) -> Value {
    let mut detail = json!({"error":error.code});
    if let Some(fields) = error.detail {
        if let Some(values) = fields.as_object() {
            detail.as_object_mut().unwrap().extend(values.clone());
        }
    }
    json!({"structuredContent":detail,"content":[{"type":"text","text":detail.to_string()}],"isError":true})
}

async fn rest_screenshot(
    State(app): State<App>,
    uri: Uri,
    h: HeaderMap,
) -> Result<Response, ApiError> {
    let key = authenticate(&app, &uri, &h).await?;
    if !key.enabled("observe") {
        return Err(ApiError::new(403, "forbidden_scope"));
    }
    app.mcp.touch(&app, &key, None);
    let started = Instant::now();
    let jpeg = crate::console::snapshot(&app).await;
    record(
        &app,
        Some(&key.name),
        "rest_screenshot",
        json!({}),
        jpeg.as_ref().map_or_else(|e| e.code.as_str(), |_| "ok"),
        started.elapsed().as_millis() as u64,
        Some(&key.id),
    );
    Ok(([(header::CONTENT_TYPE, "image/jpeg")], jpeg?).into_response())
}
async fn rest_hid(
    State(app): State<App>,
    uri: Uri,
    h: HeaderMap,
    JsonBody(body): JsonBody,
) -> Result<StatusCode, ApiError> {
    let key = authenticate(&app, &uri, &h).await?;
    if !key.enabled("input") {
        return Err(ApiError::new(403, "forbidden_scope"));
    }
    let permit = app.mcp.permit(&app, true)?;
    let _guard = app.mcp.input.lock().await;
    permit.check()?;
    let _activity = app.mcp.active_call(&app, &key, "rest_hid");
    let started = Instant::now();
    if body["t"] == "ping" || validate("ClientMessage", &body).is_err() {
        return Err(ApiError::new(400, "bad_message"));
    }
    let previous = app
        .mcp
        .raw_input
        .lock()
        .unwrap()
        .as_ref()
        .map(|(_, id)| id.clone());
    if previous.as_deref() != Some(&key.id) {
        if previous.is_some() {
            operations::release_raw(&app).await;
        }
        operations::begin(&app).await?;
    }
    *app.mcp.raw_input.lock().unwrap() = Some((Instant::now(), key.id.clone()));
    let current = || {
        app.system
            .snapshot()
            .mcp_keys
            .iter()
            .any(|item| item.id == key.id && item.enabled("input"))
    };
    let result = crate::console::input(&app, &body, || permit.running() && current())
        .await
        .and_then(|_| permit.check());
    if result.is_err() {
        operations::release_raw(&app).await;
    }
    record(
        &app,
        Some(&key.name),
        "rest_hid",
        summarize(&body),
        result.as_ref().map_or_else(|e| e.code.as_str(), |_| "ok"),
        started.elapsed().as_millis() as u64,
        None,
    );
    Ok(result.map(|_| StatusCode::NO_CONTENT)?)
}

fn summarize(v: &Value) -> Value {
    fn redact(value: &Value) -> Value {
        match value {
            Value::String(text) => json!({"bytes":text.len()}),
            Value::Array(items) => json!({"count":items.len()}),
            Value::Object(items) => Value::Object(
                items
                    .iter()
                    .filter(|(key, _)| *key != "confirm_token")
                    .map(|(key, val)| {
                        let val = if matches!(
                            key.as_str(),
                            "action"
                                | "mode"
                                | "start"
                                | "target"
                                | "until"
                                | "encoding"
                                | "button"
                        ) {
                            val.clone()
                        } else {
                            redact(val)
                        };
                        (key.clone(), val)
                    })
                    .collect(),
            ),
            _ => value.clone(),
        }
    }
    redact(v)
}

struct Audit {
    path: PathBuf,
    observations: BTreeMap<String, Value>,
}
impl Audit {
    fn new(dir: &FsPath) -> Self {
        Self {
            path: dir.join("mcp-audit.jsonl"),
            observations: BTreeMap::new(),
        }
    }
    fn flush(&mut self, minute: u64) -> io::Result<()> {
        let ready: Vec<_> = self
            .observations
            .iter()
            .filter(|(_, value)| value["ts"].as_u64().unwrap_or(0) / 60_000 < minute)
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        for (key, entry) in ready {
            self.append(&entry)?;
            self.observations.remove(&key);
        }
        Ok(())
    }
    fn append(&self, entry: &Value) -> io::Result<()> {
        let mut row = serde_json::to_vec(entry)?;
        row.push(b'\n');
        if fs::metadata(&self.path).is_ok_and(|m| m.len() + row.len() as u64 > 1_048_576) {
            let older = self.path.with_extension("jsonl.2");
            let old = self.path.with_extension("jsonl.1");
            if old.exists() {
                fs::rename(old.as_path(), older)?;
            }
            fs::rename(&self.path, old)?;
        }
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&self.path)?
            .write_all(&row)
    }
    fn read(&self, limit: usize) -> Vec<Value> {
        let mut entries: Vec<_> = self.observations.values().cloned().collect();
        for path in [
            self.path.clone(),
            self.path.with_extension("jsonl.1"),
            self.path.with_extension("jsonl.2"),
        ] {
            if let Ok(data) = fs::read_to_string(path) {
                entries.extend(
                    data.lines()
                        .rev()
                        .take(limit)
                        .filter_map(|line| serde_json::from_str::<Value>(line).ok()),
                );
            }
        }
        entries.sort_by_key(|v| std::cmp::Reverse(v["ts"].as_u64()));
        entries.truncate(limit);
        entries
    }
}
fn record(
    app: &App,
    key_name: Option<&str>,
    tool: &str,
    args: Value,
    result: &str,
    elapsed_ms: u64,
    key_id: Option<&str>,
) {
    let ts = crate::console::now_ms();
    let mut audit = app.mcp.audit.lock().unwrap();
    let entry = json!({"ts":ts,"key_name":key_name,"tool":tool,"args":args,"result":result,"elapsed_ms":elapsed_ms,"count":1});
    let out = audit.flush(ts / 60_000).and_then(|_| {
        if let Some(id) = key_id {
            if let Some(value) = audit.observations.get_mut(id) {
                value["count"] = json!(value["count"].as_u64().unwrap_or(1) + 1);
                value["elapsed_ms"] = json!(value["elapsed_ms"].as_u64().unwrap_or(0) + elapsed_ms);
                if result != "ok" {
                    value["result"] = json!(result);
                }
            } else {
                audit.observations.insert(id.into(), entry);
            }
            Ok(())
        } else {
            audit.append(&entry)
        }
    });
    if let Err(e) = out {
        eprintln!("kvmd MCP audit: {e}");
    }
}
async fn audit(
    State(app): State<App>,
    h: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<BTreeMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    app.require(&h, "mcp")?;
    let limit = q
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(100)
        .clamp(1, 1000);
    Ok(Json(
        json!({"entries":app.mcp.audit.lock().unwrap().read(limit)}),
    ))
}

pub(crate) fn valid_human_input(app: &App, msg: &Value) -> bool {
    let key_valid = |v: &Value| v.as_str().and_then(crate::hardware::hid::keycode).is_some();
    match msg["t"].as_str() {
        Some("key") => key_valid(&msg["code"]),
        Some("shortcut") => msg["keys"]
            .as_array()
            .is_some_and(|keys| keys.iter().all(key_valid)),
        Some("paste") => msg["text"].as_str().is_some_and(|text| {
            text.len() <= 65536
                && crate::hardware::layout::strokes(
                    app.config()["hid"]["keyboard_layout"]
                        .as_str()
                        .unwrap_or("us"),
                    text,
                    false,
                )
                .is_ok()
        }),
        Some("mouse_abs") => app.applied_usb()["mouse_abs"] == true,
        Some("mouse_rel") => app.applied_usb()["mouse_rel"] == true,
        Some("wheel") => {
            app.applied_usb()["mouse_rel"] == true || app.applied_usb()["mouse_abs"] == true
        }
        _ => false,
    }
}

pub(crate) async fn release_raw_input(app: &App) {
    operations::release_raw(app).await;
}

pub(crate) fn flush_audit(app: &App) {
    let _ = app.mcp.audit.lock().unwrap().flush(u64::MAX);
}

pub(crate) async fn background(app: App) {
    observe::background(&app).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmations_bind_key_parameters_and_board_state_and_expire() {
        let dir = tempfile::tempdir().unwrap();
        let mcp = Mcp::new(dir.path());
        let key = Key::migrated("hash".into(), "0000".into());
        let other = Key::migrated("hash".into(), "1111".into());
        let args = json!({"action":"eject"});
        let state = json!({"mounted":"/data.img"});
        let reply = mcp
            .confirm(&key, "virtual_media", &args, state.clone(), "eject")
            .unwrap()
            .unwrap();
        assert_eq!(reply["expires_in"], 30);
        let token = reply["confirm_token"].as_str().unwrap();
        let confirmed = json!({"action":"eject","confirm_token":token});
        assert!(mcp
            .confirm(&other, "virtual_media", &confirmed, state.clone(), "eject")
            .is_err());
        assert!(mcp
            .confirm(
                &key,
                "virtual_media",
                &confirmed,
                json!({"mounted":"/alternate.img"}),
                "eject"
            )
            .is_err());
        assert!(mcp
            .confirm(
                &key,
                "virtual_media",
                &json!({"action":"mount","confirm_token":token}),
                state.clone(),
                "eject"
            )
            .is_err());
        assert!(mcp
            .confirm(&key, "virtual_media", &confirmed, state.clone(), "eject")
            .unwrap()
            .is_none());
        assert!(mcp
            .confirm(&key, "virtual_media", &confirmed, state.clone(), "eject")
            .is_err());
        let reply = mcp
            .confirm(&key, "virtual_media", &args, state.clone(), "eject")
            .unwrap()
            .unwrap();
        let token = reply["confirm_token"].as_str().unwrap();
        mcp.runtime
            .lock()
            .unwrap()
            .tokens
            .get_mut(token)
            .unwrap()
            .expires = Instant::now() - Duration::from_secs(1);
        assert!(mcp
            .confirm(
                &key,
                "virtual_media",
                &json!({"action":"eject","confirm_token":token}),
                state,
                "eject"
            )
            .is_err());
    }

    #[tokio::test]
    async fn human_takes_over_input_and_emergency_stop_interrupts_other_tools() {
        let dir = tempfile::tempdir().unwrap();
        let app = App::new(dir.path(), true).unwrap();
        app.system
            .change(|s| {
                s.mcp_enabled = true;
                Ok(())
            })
            .await
            .unwrap();
        let input = app.mcp.permit(&app, true).unwrap();
        let observe = app.mcp.permit(&app, false).unwrap();
        assert!(app.mcp.human_input(&app));
        assert_eq!(input.interrupted().await.code, "human_took_over");
        assert!(observe.running());
        stop_service(&app, true).await.unwrap();
        assert_eq!(observe.interrupted().await.code, "stopped");
        assert!(!app.system.snapshot().mcp_enabled);
    }

    #[test]
    fn audit_merges_by_key_and_rotates_two_archives() {
        let dir = tempfile::tempdir().unwrap();
        let app = App::new(dir.path(), true).unwrap();
        for _ in 0..3 {
            record(
                &app,
                Some("a"),
                "screenshot",
                json!({}),
                "ok",
                1,
                Some("key-id"),
            );
        }
        let mut audit = app.mcp.audit.lock().unwrap();
        assert_eq!(audit.read(10).len(), 1);
        assert_eq!(audit.read(10)[0]["count"], 3);
        audit.flush(u64::MAX).unwrap();
        for i in 0..3 {
            fs::write(&audit.path, vec![b' '; 1_048_570]).unwrap();
            audit
                .append(&json!({"ts":i,"tool":"power","result":"ok"}))
                .unwrap();
        }
        assert!(audit.path.with_extension("jsonl.1").exists());
        assert!(audit.path.with_extension("jsonl.2").exists());
        assert!(!audit.path.with_extension("jsonl.3").exists());
        assert_eq!(audit.read(1)[0]["ts"], 2);
    }

    #[test]
    fn legacy_key_migration_preserves_allowed_scopes_and_enabled_state() {
        let dir = tempfile::tempdir().unwrap();
        let raw = "legacy-key-sample";
        fs::write(
            dir.path().join("system.json"),
            json!({"mcp_enabled":true,"key_hash":digest(raw),"key_hint":"mple"}).to_string(),
        )
        .unwrap();
        let app = App::new(dir.path(), true).unwrap();
        let state = app.system.snapshot();
        assert!(state.mcp_enabled);
        assert!(state.mcp_input_policy.human_input_priority);
        assert_eq!(state.mcp_input_policy.human_input_wait_s, 2);
        let key = &state.mcp_keys[0];
        assert_eq!(key.hash, digest(raw));
        assert_eq!(key.name, "default");
        assert!(key.enabled("observe") && key.enabled("input") && !key.enabled("power"));
        assert!(fs::read_to_string(dir.path().join("system.json"))
            .unwrap()
            .contains("mcp_keys"));
    }
}
