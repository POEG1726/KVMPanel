use super::*;
use axum::{
    extract::Path as Param,
    http::header::{COOKIE, SET_COOKIE},
    routing::{patch, post},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use tokio::time::{Duration, Instant};
use uuid::Uuid;

const PERMISSIONS: [&str; 12] = [
    "console",
    "power",
    "media",
    "terminal",
    "files",
    "device",
    "network",
    "updates",
    "mcp",
    "tailscale",
    "panel",
    "usb",
];
#[derive(Clone, Serialize, Deserialize)]
struct Account {
    user: Value,
    hash: String,
}
fn password_input(password: &str) -> String {
    hex::encode(Sha256::digest(password.as_bytes()))
}
fn hash(password: &str) -> io::Result<String> {
    bcrypt::hash(password_input(password), 10).map_err(io::Error::other)
}
fn matches(password: &str, encoded: &str) -> bool {
    bcrypt::verify(password_input(password), encoded).unwrap_or(false)
}
fn account(
    name: &str,
    role: &Value,
    permissions: &Value,
    view_only: bool,
    must_change: bool,
    hash: String,
) -> Account {
    Account {
        user: json!({"name":name,"role":role,"permissions":permissions.as_array().cloned().unwrap_or_default(),"view_only":view_only,"disabled":false,"must_change_password":must_change}),
        hash,
    }
}
fn public(acc: &Account) -> Value {
    let mut user = acc.user.clone();
    if user["role"] == "admin" {
        user["permissions"] = json!(PERMISSIONS);
    }
    user
}
#[derive(Default)]
struct Guard {
    failures: u32,
    until: Option<Instant>,
}
pub struct Auth {
    accounts: BTreeMap<String, Account>,
    sessions: HashMap<String, String>,
    guards: HashMap<String, Guard>,
}
impl Auth {
    pub fn open(dir: &Path) -> io::Result<Self> {
        let accounts = load_or_create(&dir.join("accounts.json"), || {
            Ok(BTreeMap::from([(
                "admin".into(),
                account(
                    "admin",
                    &json!("admin"),
                    &json!(PERMISSIONS),
                    false,
                    true,
                    hash("admin")?,
                ),
            )]))
        })?;
        Ok(Self {
            accounts,
            sessions: HashMap::new(),
            guards: HashMap::new(),
        })
    }
    fn issue(&mut self, name: &str) -> String {
        let token = Uuid::new_v4().simple().to_string() + &Uuid::new_v4().simple().to_string();
        self.sessions.insert(token.clone(), name.to_owned());
        format!("kvm_session={token}; Path=/; HttpOnly; SameSite=Strict")
    }
    fn revoke(&mut self, name: &str) {
        self.sessions.retain(|_, user| user != name)
    }
}
fn token(h: &HeaderMap) -> &str {
    h.get_all(COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|s| s.split(';'))
        .find_map(|p| p.trim().strip_prefix("kvm_session="))
        .unwrap_or("")
}
pub fn permitted(u: &Value, p: &str) -> bool {
    p.is_empty()
        || u["role"] == "admin"
        || u["permissions"]
            .as_array()
            .is_some_and(|v| v.iter().any(|x| x == p))
}
impl App {
    pub fn require(&self, h: &HeaderMap, p: &str) -> Result<Value, ApiError> {
        let auth = self.auth.lock().unwrap();
        let acc = auth
            .sessions
            .get(token(h))
            .and_then(|name| auth.accounts.get(name))
            .filter(|a| a.user["disabled"] == false)
            .ok_or(ApiError::new(401, "unauthenticated"))?;
        let u = public(acc);
        if !permitted(&u, p) {
            return Err(ApiError::new(403, "forbidden"));
        }
        Ok(u)
    }
    fn save_accounts(
        &self,
        auth: &mut Auth,
        next: BTreeMap<String, Account>,
    ) -> Result<(), ApiError> {
        save(&self.data_dir.join("accounts.json"), &next)?;
        auth.accounts = next;
        Ok(())
    }
}
pub fn routes() -> Router<App> {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/me", get(me))
        .route("/api/auth/logout", post(logout))
        .route("/api/users", get(list).post(create))
        .route("/api/users/{name}", patch(update).delete(remove))
        .route("/api/users/me/password", post(password))
}
fn text<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap()
}
async fn encode(p: &str) -> Result<String, ApiError> {
    let p = p.to_owned();
    tokio::task::spawn_blocking(move || hash(&p))
        .await
        .map_err(|_| ApiError::new(500, "internal"))?
        .map_err(Into::into)
}
async fn verify(p: &str, hash: String) -> Result<bool, ApiError> {
    let p = p.to_owned();
    tokio::task::spawn_blocking(move || matches(&p, &hash))
        .await
        .map_err(|_| ApiError::new(500, "internal"))
}
fn locked(guard: &mut Guard, now: Instant) -> Result<(), ApiError> {
    if guard.failures >= 5 {
        guard.until = Some(now + Duration::from_secs(300));
        guard.failures = 0;
    }
    if let Some(left) = guard
        .until
        .and_then(|t| t.checked_duration_since(now))
        .filter(|d| !d.is_zero())
    {
        return Err(ApiError::new(429, "login_locked")
            .with_detail(json!({"retry_after_s":left.as_secs_f64().ceil() as u64})));
    }
    Ok(())
}
async fn login(State(a): State<App>, JsonBody(v): JsonBody) -> Result<Response, ApiError> {
    validate("login", &v)?;
    let name = text(&v, "username");
    let stored = {
        let mut auth = a.auth.lock().unwrap();
        locked(auth.guards.entry(name.into()).or_default(), Instant::now())?;
        auth.accounts.get(name).map(|acc| acc.hash.clone())
    };
    let valid = if let Some(h) = stored.clone() {
        verify(text(&v, "password"), h).await?
    } else {
        false
    };
    let delay = {
        let mut auth = a.auth.lock().unwrap();
        // Recheck after bcrypt: simultaneous attempts cannot bypass a lock or password change.
        locked(auth.guards.entry(name.into()).or_default(), Instant::now())?;
        if valid {
            if let Some(acc) = auth
                .accounts
                .get(name)
                .filter(|acc| Some(&acc.hash) == stored.as_ref())
            {
                let user = public(acc);
                auth.guards.remove(name);
                if user["disabled"] == true {
                    return Err(ApiError::new(403, "user_disabled"));
                }
                return Ok(
                    ([(SET_COOKIE, auth.issue(name))], Json(json!({"user":user}))).into_response(),
                );
            }
        }
        let guard = auth.guards.get_mut(name).unwrap();
        guard.failures += 1;
        guard.failures >= 3
    };
    if delay {
        tokio::time::sleep(Duration::from_secs(1)).await
    }
    Err(ApiError::new(401, "invalid_credentials"))
}
async fn logout(State(a): State<App>, h: HeaderMap) -> impl IntoResponse {
    a.auth.lock().unwrap().sessions.remove(token(&h));
    a.emit("sessions_changed");
    (
        StatusCode::NO_CONTENT,
        [(
            SET_COOKIE,
            "kvm_session=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict",
        )],
    )
}
async fn me(State(a): State<App>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({"user":a.require(&h,"")?})))
}
async fn list(State(a): State<App>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    a.require(&h, "admin")?;
    Ok(Json(
        json!({"users":a.auth.lock().unwrap().accounts.values().map(public).collect::<Vec<_>>()}),
    ))
}
async fn create(
    State(a): State<App>,
    h: HeaderMap,
    JsonBody(v): JsonBody,
) -> Result<impl IntoResponse, ApiError> {
    a.require(&h, "admin")?;
    validate("createUser", &v)?;
    let name = text(&v, "name");
    let acc = account(
        name,
        &v["role"],
        &v["permissions"],
        v["view_only"] == true,
        false,
        encode(text(&v, "password")).await?,
    );
    a.require(&h, "admin")?;
    let mut auth = a.auth.lock().unwrap();
    if auth.accounts.contains_key(name) {
        return Err(ApiError::new(409, "user_exists"));
    }
    let out = public(&acc);
    let mut next = auth.accounts.clone();
    next.insert(name.to_owned(), acc);
    a.save_accounts(&mut auth, next)?;
    Ok((StatusCode::CREATED, Json(out)))
}
async fn update(
    State(a): State<App>,
    h: HeaderMap,
    Param(name): Param<String>,
    JsonBody(v): JsonBody,
) -> Result<Json<Value>, ApiError> {
    a.require(&h, "admin")?;
    validate("updateUser", &v)?;
    let new_hash = if let Some(p) = v["password"].as_str() {
        Some(encode(p).await?)
    } else {
        None
    };
    a.require(&h, "admin")?;
    let mut auth = a.auth.lock().unwrap();
    let mut next = auth.accounts.clone();
    let acc = next.get_mut(&name).ok_or(ApiError::new(404, "not_found"))?;
    for key in ["permissions", "role", "disabled", "view_only"] {
        if let Some(val) = v.get(key) {
            acc.user[key] = val.clone()
        }
    }
    let revoke = new_hash.is_some() || v["disabled"] == true;
    if let Some(hash) = new_hash {
        acc.hash = hash
    }
    let out = public(acc);
    a.save_accounts(&mut auth, next)?;
    if revoke {
        auth.revoke(&name)
    }
    a.emit("sessions_changed");
    Ok(Json(out))
}
async fn remove(
    State(a): State<App>,
    h: HeaderMap,
    Param(name): Param<String>,
) -> Result<StatusCode, ApiError> {
    if a.require(&h, "admin")?["name"] == name {
        return Err(ApiError::new(409, "cannot_delete_self"));
    }
    let mut auth = a.auth.lock().unwrap();
    let mut next = auth.accounts.clone();
    next.remove(&name).ok_or(ApiError::new(404, "not_found"))?;
    a.save_accounts(&mut auth, next)?;
    auth.revoke(&name);
    a.emit("sessions_changed");
    Ok(StatusCode::NO_CONTENT)
}
async fn password(
    State(a): State<App>,
    h: HeaderMap,
    JsonBody(v): JsonBody,
) -> Result<impl IntoResponse, ApiError> {
    let u = a.require(&h, "")?;
    validate("changePassword", &v)?;
    let name = text(&u, "name");
    let old = a
        .auth
        .lock()
        .unwrap()
        .accounts
        .get(name)
        .ok_or(ApiError::new(401, "unauthenticated"))?
        .hash
        .clone();
    if !verify(text(&v, "old"), old.clone()).await? {
        return Err(ApiError::new(403, "wrong_password"));
    }
    let hash = encode(text(&v, "new")).await?;
    let mut auth = a.auth.lock().unwrap();
    if auth.sessions.get(token(&h)).map(String::as_str) != Some(name) {
        return Err(ApiError::new(401, "unauthenticated"));
    }
    let mut next = auth.accounts.clone();
    let acc = next
        .get_mut(name)
        .ok_or(ApiError::new(401, "unauthenticated"))?;
    if acc.hash != old {
        return Err(ApiError::new(403, "wrong_password"));
    }
    acc.hash = hash;
    acc.user["must_change_password"] = json!(false);
    a.save_accounts(&mut auth, next)?;
    auth.revoke(name);
    a.emit("sessions_changed");
    Ok((StatusCode::NO_CONTENT, [(SET_COOKIE, auth.issue(name))]))
}
