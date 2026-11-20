use super::*;
use axum::routing::post;

pub struct ConfigState {
    value: Value,
    applied: Value,
}
impl ConfigState {
    pub fn open(dir: &Path) -> io::Result<Self> {
        let value: Value =
            load_or_create(&dir.join("config.json"), || Ok(schema::default_config()))?;
        validate("Config", &value).map_err(|_| io::Error::other("invalid stored config"))?;
        Ok(Self {
            applied: value["usb"].clone(),
            value,
        })
    }
}
pub fn usb_budget(u: &Value) -> Value {
    let on = |key: &str| u[key] == true;
    let rel = on("mouse_rel");
    let abs = on("mouse_abs");
    let dual = on("net") as usize + on("acm") as usize;
    let input = on("keyboard") as usize
        + rel as usize
        + (abs && !(rel && on("mouse_merge"))) as usize
        + (on("telemetry") && !rel && !abs) as usize
        + on("storage") as usize
        + 2 * dual;
    let output = on("storage") as usize + dual;
    json!({"in":input,"out":output,"fits":input<=5 && output<=5,"in_missing":input.saturating_sub(5),"out_missing":output.saturating_sub(5)})
}
fn merge(target: &mut Value, patch: &Value, path: &str) {
    if let (Some(obj), Some(fields)) = (target.as_object_mut(), patch.as_object()) {
        for (key, val) in fields {
            if path == "/panel/thresholds" && val.is_null() {
                obj.remove(key);
            } else {
                merge(
                    obj.entry(key).or_insert(Value::Null),
                    val,
                    &format!("{path}/{key}"),
                );
            }
        }
    } else {
        target.clone_from(patch);
    }
}
impl App {
    pub fn config(&self) -> Value {
        self.cfg.lock().unwrap().value.clone()
    }
    pub fn patch_config(&self, patch: &Value, account: Option<&Value>) -> Result<Value, ApiError> {
        let mut cfg = self.cfg.lock().unwrap();
        let mut next = cfg.value.clone();
        merge(&mut next, patch, "");
        validate("Config", &next)?;
        if let Some(user) = account {
            for key in patch.as_object().unwrap().keys() {
                let perm = match key.as_str() {
                    "panel" | "function_key" => "panel",
                    "usb" => "usb",
                    "video" | "hid" => "console",
                    "serial" => "terminal",
                    _ => "device",
                };
                if !auth::permitted(user, perm) {
                    return Err(ApiError::new(403, "forbidden"));
                }
            }
        }
        let budget = usb_budget(&next["usb"]);
        if budget["fits"] != true {
            return Err(ApiError::new(409, "endpoints_full").with_detail(
                json!({"in_missing":budget["in_missing"],"out_missing":budget["out_missing"]}),
            ));
        }
        save(&self.data_dir.join("config.json"), &next)?;
        cfg.value = next.clone();
        self.emit("config_changed");
        Ok(next)
    }
    pub fn apply_usb(&self) -> Result<(), ApiError> {
        let mut cfg = self.cfg.lock().unwrap();
        self.hw.apply_usb(&cfg.value["usb"], &[])?;
        cfg.applied = cfg.value["usb"].clone();
        self.emit("usb_reenumerated");
        Ok(())
    }
}
pub fn routes() -> Router<App> {
    Router::new()
        .route("/api/config", get(get_config).patch(patch))
        .route("/api/usb/budget", get(budget))
        .route("/api/usb/applied", get(applied))
        .route("/api/usb/apply", post(apply))
}
async fn get_config(State(a): State<App>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    a.require(&h, "")?;
    Ok(Json(a.config()))
}
async fn patch(
    State(a): State<App>,
    h: HeaderMap,
    JsonBody(v): JsonBody,
) -> Result<Json<Value>, ApiError> {
    let user = a.require(&h, "")?;
    Ok(Json(a.patch_config(&v, Some(&user))?))
}
async fn budget(State(a): State<App>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    a.require(&h, "")?;
    Ok(Json(usb_budget(&a.config()["usb"])))
}
async fn applied(State(a): State<App>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    a.require(&h, "")?;
    let cfg = a.cfg.lock().unwrap();
    Ok(Json(
        json!({"usb":cfg.applied,"pending":cfg.applied!=cfg.value["usb"]}),
    ))
}
async fn apply(State(a): State<App>, h: HeaderMap) -> Result<impl IntoResponse, ApiError> {
    a.require(&h, "usb")?;
    a.apply_usb()?;
    Ok((StatusCode::ACCEPTED, Json(json!({"reenumerating":true}))))
}
