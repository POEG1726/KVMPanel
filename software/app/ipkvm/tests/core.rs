mod common;
use common::{cookie, Server};
use kvmd::{validate, App};
use serde_json::{json, Value};
use tokio::time::{Duration, Instant};

async fn check(
    s: &Server,
    method: &str,
    path: &str,
    auth: &str,
    body: Option<Value>,
    code: u16,
    op: &str,
) -> Value {
    let response = s.call(method, path, auth, body).await;
    assert_eq!(response.status(), code, "{method} {path}");
    if code == 204 {
        return Value::Null;
    }
    let v = response.json().await.unwrap();
    validate(&format!("{op}/{code}"), &v).unwrap();
    v
}

#[tokio::test]
async fn health_config_permissions_and_usb() {
    let s = Server::new().await;
    assert_eq!(
        check(&s, "GET", "/api/health", "", None, 200, "getHealth").await,
        json!({"ok":true,"version":"0.1.0"})
    );
    assert!(validate("Health", &json!({"ok":"yes"})).is_err());
    let method = s.call("POST", "/api/health", "", None).await;
    assert_eq!(method.status(), 405);
    assert_eq!(
        method.json::<Value>().await.unwrap(),
        json!({"error":"method_not_allowed"})
    );
    let a = s.login("admin", "admin").await;
    let original = check(&s, "GET", "/api/config", &a, None, 200, "getConfig").await;
    assert_eq!(
        original,
        json!({"panel":{"language":"en","auto_brightness":true,"brightness":70,"offline_sleep_min":5,"thresholds":{"cpu.tctl":85,"gpu.mem_junction":95,"mem.dimm.*":60},"hidden_metrics":[]},"function_key":{"short":"toggle_settings","long":"toggle_screen"},"usb":{"keyboard":true,"mouse_rel":true,"mouse_abs":true,"mouse_merge":false,"storage":true,"net":false,"net_mode":"ncm","acm":false,"telemetry":true},"web":{"language":"auto","workspace_open":"float","remember_float":true,"menubar":"auto_hide","title":"KVMPanel"},"video":{"quality":80,"fps_max":30,"scale":"fit","frame_diff":true},"hid":{"keyboard_layout":"us","mouse_mode":"abs","wheel_dir":"normal","wheel_speed":1,"jiggler":false,"leader_key":null},"serial":{"device":null,"baud":115200},"device":{"hostname":"kvmpanel","ssh":true,"mdns":true,"watchdog":false},"kvm":{"enabled":true}})
    );
    assert_eq!(
        check(&s, "GET", "/api/usb/budget", &a, None, 200, "getUsbBudget").await,
        json!({"in":4,"out":1,"fits":true,"in_missing":0,"out_missing":0})
    );
    for (v, p) in [
        (json!({"panel":{"brightness":200}}), "/panel/brightness"),
        (json!({"panel":{"nope":1}}), "/panel/nope"),
        (json!({"nosuch":{}}), "/nosuch"),
    ] {
        assert_eq!(
            check(&s, "PATCH", "/api/config", &a, Some(v), 400, "patchConfig").await,
            json!({"error":"invalid_config","detail":{"path":p}})
        );
    }
    assert_eq!(
        check(
            &s,
            "PATCH",
            "/api/config",
            &a,
            Some(json!({"usb":{"net":true}})),
            409,
            "patchConfig"
        )
        .await,
        json!({"error":"endpoints_full","detail":{"in_missing":1,"out_missing":0}})
    );
    assert_eq!(*s.app.config(), original);
    assert_eq!(
        check(
            &s,
            "GET",
            "/api/usb/applied",
            &a,
            None,
            200,
            "getUsbApplied"
        )
        .await["pending"],
        false
    );
    let changed=check(&s,"PATCH","/api/config",&a,Some(json!({"usb":{"mouse_abs":false},"panel":{"brightness":30,"thresholds":{"gpu.temp":80}}})),200,"patchConfig").await;
    assert_eq!(changed["panel"]["brightness"], 30);
    assert_eq!(changed["panel"]["thresholds"]["gpu.temp"], 80);
    assert!(check(
        &s,
        "PATCH",
        "/api/config",
        &a,
        Some(json!({"panel":{"thresholds":{"gpu.temp":null}}})),
        200,
        "patchConfig"
    )
    .await["panel"]["thresholds"]
        .get("gpu.temp")
        .is_none());
    check(
        &s,
        "PATCH",
        "/api/config",
        &a,
        Some(json!({"panel":{"thresholds":{"Bad Key":1}}})),
        400,
        "patchConfig",
    )
    .await;
    let pending = check(
        &s,
        "GET",
        "/api/usb/applied",
        &a,
        None,
        200,
        "getUsbApplied",
    )
    .await;
    assert_eq!(pending["pending"], true);
    assert_eq!(pending["usb"]["mouse_abs"], true);
    let mut events = s.app.events.subscribe();
    assert_eq!(
        check(&s, "POST", "/api/usb/apply", &a, None, 202, "applyUsb").await,
        json!({"reenumerating":true})
    );
    assert_eq!(events.recv().await.unwrap(), "usb_reenumerated");
    let applied = check(
        &s,
        "GET",
        "/api/usb/applied",
        &a,
        None,
        200,
        "getUsbApplied",
    )
    .await;
    assert_eq!(applied["pending"], false);
    assert_eq!(applied["usb"]["mouse_abs"], false);
    let restored = App::new(s.dir.path(), true).unwrap();
    assert_eq!(restored.config(), s.app.config());
    let bad = s
        .client
        .patch(format!("{}/api/config", s.url))
        .header("cookie", &a)
        .header("content-type", "application/json")
        .body("{nope")
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), 400);
    assert_eq!(
        bad.json::<Value>().await.unwrap(),
        json!({"error":"invalid_json"})
    );
    let missing = s.call("GET", "/api/does-not-exist", &a, None).await;
    assert_eq!(missing.status(), 404);
    assert_eq!(
        missing.json::<Value>().await.unwrap(),
        json!({"error":"not_found"})
    );
    std::fs::create_dir(s.dir.path().join("web")).unwrap();
    std::fs::write(s.dir.path().join("web/index.html"), "web").unwrap();
    assert_eq!(
        s.call("GET", "/", "", None).await.text().await.unwrap(),
        "web"
    );
}

#[tokio::test]
async fn account_lifecycle() {
    let s = Server::new().await;
    assert_eq!(
        check(&s, "GET", "/api/auth/me", "", None, 401, "getMe").await["error"],
        "unauthenticated"
    );
    let a = s.login("admin", "admin").await;
    let b = s.login("admin", "admin").await;
    let me = check(&s, "GET", "/api/auth/me", &a, None, 200, "getMe").await;
    assert_eq!(me["user"]["must_change_password"], true);
    let v = json!({"name":"guest","password":"guestpw","role":"user","permissions":["console","panel"],"view_only":true});
    check(
        &s,
        "POST",
        "/api/users",
        &a,
        Some(v.clone()),
        201,
        "createUser",
    )
    .await;
    assert_eq!(
        check(&s, "POST", "/api/users", &a, Some(v), 409, "createUser").await["error"],
        "user_exists"
    );
    let users = check(&s, "GET", "/api/users", &a, None, 200, "listUsers").await;
    assert_eq!(users["users"][0]["name"], "admin");
    assert_eq!(users["users"][1]["name"], "guest");
    let g = s.login("guest", "guestpw").await;
    assert_eq!(
        check(&s, "GET", "/api/users", &g, None, 403, "listUsers").await["error"],
        "forbidden"
    );
    check(
        &s,
        "PATCH",
        "/api/config",
        &g,
        Some(json!({"usb":{"acm":false}})),
        403,
        "patchConfig",
    )
    .await;
    check(
        &s,
        "PATCH",
        "/api/config",
        &g,
        Some(json!({"panel":{"brightness":10}})),
        200,
        "patchConfig",
    )
    .await;
    check(
        &s,
        "PATCH",
        "/api/users/guest",
        &a,
        Some(json!({"disabled":true})),
        200,
        "updateUser",
    )
    .await;
    check(&s, "GET", "/api/auth/me", &g, None, 401, "getMe").await;
    assert_eq!(
        check(
            &s,
            "POST",
            "/api/auth/login",
            "",
            Some(json!({"username":"guest","password":"guestpw"})),
            403,
            "login"
        )
        .await["error"],
        "user_disabled"
    );
    assert_eq!(
        check(
            &s,
            "DELETE",
            "/api/users/admin",
            &a,
            None,
            409,
            "deleteUser"
        )
        .await["error"],
        "cannot_delete_self"
    );
    check(
        &s,
        "DELETE",
        "/api/users/guest",
        &a,
        None,
        204,
        "deleteUser",
    )
    .await;
    check(
        &s,
        "DELETE",
        "/api/users/guest",
        &a,
        None,
        404,
        "deleteUser",
    )
    .await;
    assert_eq!(
        check(
            &s,
            "POST",
            "/api/users/me/password",
            &a,
            Some(json!({"old":"nope","new":"abcdef"})),
            403,
            "changePassword"
        )
        .await["error"],
        "wrong_password"
    );
    let changed = s
        .call(
            "POST",
            "/api/users/me/password",
            &a,
            Some(json!({"old":"admin","new":"n3w-pass"})),
        )
        .await;
    assert_eq!(changed.status(), 204);
    let fresh = cookie(&changed);
    for old in [&a, &b] {
        check(&s, "GET", "/api/auth/me", old, None, 401, "getMe").await;
    }
    assert_eq!(
        check(&s, "GET", "/api/auth/me", &fresh, None, 200, "getMe").await["user"]
            ["must_change_password"],
        false
    );
    check(&s, "POST", "/api/auth/logout", &fresh, None, 204, "logout").await;
    check(&s, "GET", "/api/auth/me", &fresh, None, 401, "getMe").await;
    assert!(!std::fs::read_to_string(s.dir.path().join("accounts.json"))
        .unwrap()
        .contains("n3w-pass"));
}

#[tokio::test]
async fn backoff_lock_and_expiry() {
    let s = Server::new().await;
    for i in 1..=5 {
        let start = Instant::now();
        assert_eq!(
            check(
                &s,
                "POST",
                "/api/auth/login",
                "",
                Some(json!({"username":"admin","password":"wrong"})),
                401,
                "login"
            )
            .await["error"],
            "invalid_credentials"
        );
        if i >= 3 {
            assert!(start.elapsed() >= Duration::from_secs(1));
        }
    }
    for password in ["wrong", "admin"] {
        let r = check(
            &s,
            "POST",
            "/api/auth/login",
            "",
            Some(json!({"username":"admin","password":password})),
            429,
            "login",
        )
        .await;
        assert_eq!(
            r,
            json!({"error":"login_locked","detail":{"retry_after_s":300}})
        );
    }
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(301)).await;
    tokio::time::resume();
    s.login("admin", "admin").await;
}

#[tokio::test]
async fn endpoint_combinations() {
    let s = Server::new().await;
    let base = s.app.config()["usb"].clone();
    for (patch, input, output) in [
        (json!({"net":true}), 6, 2),
        (json!({"mouse_merge":true}), 3, 1),
        (json!({"mouse_merge":true,"net":true}), 5, 2),
        (
            json!({"mouse_abs":false,"mouse_rel":false,"storage":false}),
            2,
            0,
        ),
        (
            json!({"mouse_abs":false,"mouse_rel":false,"storage":false,"telemetry":false}),
            1,
            0,
        ),
        (
            json!({"mouse_rel":false,"mouse_abs":false,"storage":false,"net":true,"acm":true}),
            6,
            2,
        ),
        (json!({"mouse_abs":false,"mouse_merge":true}), 3, 1),
    ] {
        let mut u = base.clone();
        u.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        let r = kvmd::usb_budget(&u);
        assert_eq!(r["in"], input);
        assert_eq!(r["out"], output);
        assert_eq!(r["fits"], input <= 5);
    }
}

#[tokio::test]
async fn persisted_accounts_config_scripts_wol_mcp_and_logo_survive_restart() {
    let s = Server::new().await;
    let a = s.login("admin", "admin").await;
    let b=s.call("POST","/api/users",&a,Some(json!({"name":"reader","password":"reading","role":"user","permissions":["console"]}))).await;
    assert_eq!(b.status(), 201);
    s.app
        .patch_config(
            &json!({"panel":{"brightness":48},"usb":{"mouse_abs":false}}),
            None,
        )
        .unwrap();
    assert_eq!(
        s.call(
            "POST",
            "/api/wol/devices",
            &a,
            Some(json!({"name":"lab","mac":"02:00:00:00:00:01"}))
        )
        .await
        .status(),
        201
    );
    let put = s
        .client
        .put(format!("{}/api/scripts/sample.sh", s.url))
        .header("cookie", &a)
        .header("content-type", "text/plain")
        .body("echo sample\n")
        .send()
        .await
        .unwrap();
    assert_eq!(put.status(), 201);
    let put = s
        .client
        .put(format!("{}/api/branding/logo", s.url))
        .header("cookie", &a)
        .header("content-type", "image/svg+xml")
        .body("<svg/>")
        .send()
        .await
        .unwrap();
    assert_eq!(put.status(), 204);
    let key = s
        .call("POST", "/api/mcp/regenerate", &a, None)
        .await
        .json::<Value>()
        .await
        .unwrap()["key"]
        .as_str()
        .unwrap()
        .to_owned();
    let restarted = App::new(s.dir.path(), true).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let routes = kvmd::router(restarted, s.dir.path());
    let task = tokio::spawn(async move { axum::serve(listener, routes).await.unwrap() });
    assert_eq!(
        s.client
            .get(format!("{base}/api/auth/me"))
            .header("cookie", &a)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    for (name, pw) in [("reader", "reading"), ("admin", "admin")] {
        let login = s
            .client
            .post(format!("{base}/api/auth/login"))
            .json(&json!({"username":name,"password":pw}))
            .send()
            .await
            .unwrap();
        assert_eq!(login.status(), 200);
        if name == "admin" {
            let token = cookie(&login);
            for (path, pointer, expected) in [
                ("config", "/panel/brightness", json!(48)),
                ("usb/applied", "/pending", json!(true)),
                ("scripts", "/0/name", json!("sample.sh")),
                ("wol/devices", "/devices/0/name", json!("lab")),
            ] {
                let value = s
                    .client
                    .get(format!("{base}/api/{path}"))
                    .header("cookie", &token)
                    .send()
                    .await
                    .unwrap()
                    .json::<Value>()
                    .await
                    .unwrap();
                assert_eq!(value.pointer(pointer), Some(&expected));
            }
        }
    }
    assert_eq!(
        s.client
            .get(format!("{base}/api/branding/logo"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "<svg/>"
    );
    assert_eq!(
        s.client
            .get(format!("{base}/api/mcp/v1/screenshot"))
            .bearer_auth(&key)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    task.abort();
}
