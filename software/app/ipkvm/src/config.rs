use super::*;
use axum::routing::post;

pub struct ConfigState {
    value: RwLock<Arc<Value>>,
    applied: RwLock<Value>,
    write: Mutex<()>,
    apply: Mutex<()>,
}
impl ConfigState {
    pub fn open(dir: &Path) -> io::Result<Self> {
        let defaults = schema::default_config();
        let value = load_config_file(&dir.join("config.json"), &defaults, |stored| {
            migrate(&defaults, stored)
        })?;
        let applied = load_config_file(&dir.join("usb-applied.json"), &value["usb"], |stored| {
            migrate(&defaults, &json!({"usb":stored}))["usb"].clone()
        })?;
        Ok(Self {
            value: RwLock::new(Arc::new(value)),
            applied: RwLock::new(applied),
            write: Mutex::new(()),
            apply: Mutex::new(()),
        })
    }
}
// Only migrate persisted settings. Incoming HTTP and panel patches remain strict.
fn load_config_file(
    path: &Path,
    initial: &Value,
    repair: impl FnOnce(&Value) -> Value,
) -> io::Result<Value> {
    let old = match fs::read(path) {
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(v) => Some(v),
            Err(e) => {
                eprintln!("kvmd: restoring {} after JSON error: {e}", path.display());
                Some(Value::Null)
            }
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let new = old.as_ref().map(repair).unwrap_or_else(|| initial.clone());
    if old.as_ref() != Some(&new) {
        save(path, &new)?;
    }
    Ok(new)
}
fn migrate(defaults: &Value, stored: &Value) -> Value {
    let mut cfg = defaults.clone();
    let Some(sections) = stored.as_object() else {
        eprintln!("kvmd: invalid stored config, using defaults");
        return cfg;
    };
    for (name, section) in sections {
        let Some(base) = defaults.get(name) else {
            eprintln!("kvmd: ignoring obsolete config section {name}");
            continue;
        };
        let mut candidate = base.clone();
        if let Some(fields) = section.as_object() {
            // Open maps (e.g. thresholds) and arrays are complete saved field values.
            candidate.as_object_mut().unwrap().extend(fields.clone());
        } else {
            candidate = section.clone();
        }
        crate::normalize_numbers(&mut candidate);
        cfg[name] = candidate;
        if validate("Config", &cfg).is_err()
            || (name == "usb" && usb_budget(&cfg[name])["fits"] != true)
        {
            eprintln!("kvmd: invalid stored {name} section, using defaults");
            cfg[name] = base.clone();
        }
    }
    cfg
}
pub fn usb_budget(u: &Value) -> Value {
    let on = |key: &str| u[key] == true;
    let rel = on("mouse_rel");
    let abs = on("mouse_abs");
    let dual = on("net") as usize + on("acm") as usize;
    let input = on("keyboard") as usize
        + rel as usize
        + (abs && !(rel && on("mouse_merge"))) as usize
        + (on("telemetry") && !abs) as usize
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
    pub fn subscribe_config(&self) -> (Arc<Value>, broadcast::Receiver<Arc<Value>>) {
        let cfg = self.cfg.value.read().unwrap();
        (cfg.clone(), self.config_updates.subscribe())
    }
    pub fn config(&self) -> Arc<Value> {
        self.cfg.value.read().unwrap().clone()
    }
    pub fn storage_enabled(&self) -> bool {
        self.cfg.applied.read().unwrap()["storage"] == true
    }
    pub fn patch_config(
        &self,
        patch: &Value,
        account: Option<&Value>,
    ) -> Result<Arc<Value>, ApiError> {
        let _writer = self.cfg.write.lock().unwrap();
        let old = self.config();
        let mut next = (*old).clone();
        merge(&mut next, patch, "");
        crate::normalize_numbers(&mut next);
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
        let paused = old["kvm"]["enabled"] == true && next["kvm"]["enabled"] == false;
        let next = Arc::new(next);
        {
            let mut current = self.cfg.value.write().unwrap();
            *current = next.clone();
            let _ = self.config_updates.send(next.clone());
        }
        if paused {
            self.emit("disconnect_sessions")
        }
        self.emit("config_changed");
        Ok(next)
    }
    pub fn restore_usb(&self) -> Result<(), ApiError> {
        let _apply = self.cfg.apply.lock().unwrap();
        let applied = self.cfg.applied.read().unwrap().clone();
        self.storage
            .with_gadget_args(applied["storage"] == true, |media| {
                self.hw
                    .apply_usb(&applied, &media)
                    .map_err(ApiError::device)
            })
    }
    pub fn apply_usb(&self) -> Result<(), ApiError> {
        let _apply = self.cfg.apply.lock().unwrap();
        let desired = self.config()["usb"].clone();
        let previous = self.cfg.applied.read().unwrap().clone();
        self.storage
            .with_gadget_args(desired["storage"] == true, |media| {
                let change = self
                    .hw
                    .apply_usb(&desired, &media)
                    .map_err(ApiError::device)
                    .and_then(|()| {
                        save(&self.data_dir.join("usb-applied.json"), &desired)
                            .map_err(ApiError::from)
                    });
                if let Err(e) = change {
                    if let Err(rollback) = self.hw.apply_usb(&previous, &media) {
                        eprintln!("USB restore failed: {rollback}");
                    }
                    return Err(e);
                }
                *self.cfg.applied.write().unwrap() = desired.clone();
                Ok(())
            })?;
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
async fn get_config(State(a): State<App>, h: HeaderMap) -> Result<Json<Arc<Value>>, ApiError> {
    a.require(&h, "")?;
    Ok(Json(a.config()))
}
async fn patch(
    State(a): State<App>,
    h: HeaderMap,
    JsonBody(v): JsonBody,
) -> Result<Json<Arc<Value>>, ApiError> {
    a.require(&h, "")?;
    let next = tokio::task::spawn_blocking(move || {
        let user = a.require(&h, "")?;
        a.patch_config(&v, Some(&user))
    })
    .await
    .map_err(io::Error::other)??;
    Ok(Json(next))
}
async fn budget(State(a): State<App>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    a.require(&h, "")?;
    Ok(Json(usb_budget(&a.config()["usb"])))
}
async fn applied(State(a): State<App>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    a.require(&h, "")?;
    let mut actual = a.cfg.applied.read().unwrap().clone();
    let hw = a.hw.clone();
    let bound = tokio::task::spawn_blocking(move || hw.usb_bound())
        .await
        .map_err(io::Error::other)?
        .map_err(ApiError::device)?;
    if !bound {
        for key in [
            "keyboard",
            "mouse_rel",
            "mouse_abs",
            "telemetry",
            "storage",
            "net",
            "acm",
        ] {
            actual[key] = false.into();
        }
    }
    Ok(Json(
        json!({"usb":actual,"pending":actual!=a.config()["usb"]}),
    ))
}
async fn apply(State(a): State<App>, h: HeaderMap) -> Result<impl IntoResponse, ApiError> {
    a.require(&h, "usb")?;
    tokio::task::spawn_blocking(move || a.apply_usb())
        .await
        .map_err(io::Error::other)??;
    Ok((StatusCode::ACCEPTED, Json(json!({"reenumerating":true}))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    #[tokio::test(flavor = "current_thread")]
    async fn slow_gadget_keeps_http_and_the_requesting_panel_responsive() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("gadget.sh");
        fs::write(
            &script,
            "cd \"$(dirname \"$0\")\" || exit 1\ntouch started\nsleep 2\n",
        )
        .unwrap();
        let mut app = App::new(dir.path(), true).unwrap();
        Arc::get_mut(&mut app.0).unwrap().hw = hardware::simulated_gadget(script.clone());
        let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = tcp.local_addr().unwrap();
        let routes = router(app.clone(), dir.path());
        let web = tokio::spawn(async move { axum::serve(tcp, routes).await.unwrap() });
        let panel_app = app.clone();
        let path = dir.path().join("panel.sock");
        let sock = path.clone();
        let panel = tokio::spawn(async move {
            panel::serve(panel_app, sock.to_str().unwrap())
                .await
                .unwrap()
        });
        let stream = loop {
            match tokio::net::UnixStream::connect(&path).await {
                Ok(s) => break s,
                Err(_) => tokio::task::yield_now().await,
            }
        };
        let (read, mut writer) = stream.into_split();
        let mut lines = BufReader::new(read).lines();
        for _ in 0..3 {
            lines.next_line().await.unwrap().unwrap();
        }
        let client = reqwest::Client::new();
        let login = client
            .post(format!("http://{addr}/api/auth/login"))
            .json(&json!({"username":"admin","password":"admin"}))
            .send()
            .await
            .unwrap();
        let cookie = login.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        for from_panel in [false, true] {
            let http = if from_panel {
                writer
                    .write_all(b"{\"t\":\"action\",\"action\":\"usb_apply\",\"id\":1}\n")
                    .await
                    .unwrap();
                None
            } else {
                let req = client
                    .post(format!("http://{addr}/api/usb/apply"))
                    .header("cookie", &cookie);
                Some(tokio::spawn(req.send()))
            };
            let started = dir.path().join("started");
            tokio::time::timeout(Duration::from_secs(1), async {
                while !started.exists() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            let clock = Instant::now();
            let r = client
                .get(format!("http://{addr}/api/health"))
                .send()
                .await
                .unwrap();
            assert_eq!(r.status(), 200);
            assert!(clock.elapsed() < Duration::from_millis(200));
            let clock = Instant::now();
            writer
                .write_all(b"{\"t\":\"get_config\",\"id\":2}\n")
                .await
                .unwrap();
            loop {
                let msg: Value =
                    serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
                if msg["id"] == 2 {
                    assert_eq!(msg["data"]["panel"]["brightness"], 70);
                    break;
                }
            }
            assert!(clock.elapsed() < Duration::from_millis(200));
            // Change the desired config before the captured apply completes.
            let old = app.config()["usb"]["mouse_abs"] == true;
            app.patch_config(&json!({"usb":{"mouse_abs":!old}}), None)
                .unwrap();
            if let Some(http) = http {
                assert_eq!(http.await.unwrap().unwrap().status(), 202);
            } else {
                loop {
                    let msg: Value =
                        serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
                    if msg["id"] == 1 {
                        assert_eq!(msg["ok"], true);
                        break;
                    }
                }
            }
            assert_eq!(app.cfg.applied.read().unwrap()["mouse_abs"], old);
            fs::remove_file(started).unwrap();
        }
        fs::write(&script, "exit 1\n").unwrap();
        let r = client
            .post(format!("http://{addr}/api/usb/apply"))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 503);
        assert_eq!(
            r.json::<Value>().await.unwrap(),
            json!({"error":"device_unavailable"})
        );
        web.abort();
        panel.abort();
    }
}
