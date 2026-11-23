mod common;
use common::Server;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
    MaybeTlsStream, WebSocketStream,
};

async fn checked(r: reqwest::Response, op: &str, status: u16) -> Value {
    assert_eq!(r.status(), status, "{op}");
    let body: Value = r.json().await.unwrap();
    kvmd::validate(&format!("{op}/{status}"), &body).unwrap();
    body
}

#[tokio::test]
async fn device_network_tailscale_and_about() {
    let s = Server::new().await;
    let cookie = s.login("admin", "admin").await;
    for (op, path) in [
        ("getDevice", "device"),
        ("getNetwork", "network"),
        ("getTailscale", "tailscale"),
        ("getUpdate", "update"),
        ("getMcp", "mcp"),
    ] {
        let value = checked(
            s.call("GET", &format!("/api/{path}"), &cookie, None).await,
            op,
            200,
        )
        .await;
        if path == "device" {
            assert_eq!(value["hostname"], "kvmpanel");
            assert_eq!(value["link_mbps"], 1000);
        }
    }
    let about = checked(s.call("GET", "/api/about", "", None).await, "getAbout", 200).await;
    assert_eq!(
        about,
        json!({"name":"KVMPanel","version":"0.1.0","licenses_url":"/licenses.txt"})
    );
    let mut net = s
        .call("GET", "/api/network", &cookie, None)
        .await
        .json::<Value>()
        .await
        .unwrap();
    net["mode"] = json!("static");
    assert_eq!(
        checked(
            s.call("PUT", "/api/network", &cookie, Some(net.clone()))
                .await,
            "putNetwork",
            400
        )
        .await["error"],
        "invalid_body"
    );
    net["ip"] = json!("192.168.1.50");
    net["mask"] = json!("255.255.255.0");
    net["gateway"] = json!("192.168.1.1");
    assert_eq!(
        checked(
            s.call("PUT", "/api/network", &cookie, Some(net.clone()))
                .await,
            "putNetwork",
            200
        )
        .await,
        net
    );
    assert_eq!(
        kvmd::App::new(s.dir.path(), true).unwrap().system.network(),
        net
    );
    for a in ["up", "down", "login", "logout", "uninstall", "install"] {
        assert_eq!(
            s.call("POST", &format!("/api/tailscale/{a}"), &cookie, None)
                .await
                .status(),
            202
        );
    }
    assert_eq!(
        checked(
            s.call("POST", "/api/tailscale/explode", &cookie, None)
                .await,
            "tailscaleAction",
            400
        )
        .await["error"],
        "bad_action"
    );
    let user = limited(&s, &cookie).await;
    assert_eq!(
        checked(
            s.call(
                "POST",
                "/api/device/action",
                &user,
                Some(json!({"action":"reboot"}))
            )
            .await,
            "deviceAction",
            403
        )
        .await["error"],
        "forbidden"
    );
}
#[tokio::test]
async fn branding_mcp_keys_and_bearer() {
    let s = Server::new().await;
    let c = s.login("admin", "admin").await;
    let logo = format!("{}/api/branding/logo", s.url);
    let put = |data: Vec<u8>, kind: &str| {
        s.client
            .put(&logo)
            .header("content-type", kind)
            .header("cookie", &c)
            .body(data)
            .send()
    };
    checked(s.client.get(&logo).send().await.unwrap(), "getLogo", 404).await;
    assert_eq!(
        checked(
            put(vec![1; 300 * 1024], "image/png").await.unwrap(),
            "putLogo",
            413
        )
        .await["error"],
        "too_large"
    );
    for (kind, data) in [
        ("image/png", &b"\x89PNG\r\n\x1a\n"[..]),
        ("image/svg+xml", &b"<svg/>"[..]),
    ] {
        assert_eq!(put(data.to_vec(), kind).await.unwrap().status(), 204);
        let r = s.client.get(&logo).send().await.unwrap();
        assert_eq!(r.status(), 200);
        assert_eq!(r.headers()["content-type"], kind);
        assert_eq!(r.bytes().await.unwrap().as_ref(), data);
    }
    let enable = |v| s.call("PUT", "/api/mcp", &c, Some(json!({"enabled":v})));
    assert_eq!(
        checked(enable(true).await, "putMcp", 409).await["error"],
        "no_key"
    );
    let make_key = || {
        s.call(
            "POST",
            "/api/mcp/keys",
            &c,
            Some(json!({"name":"Integration","scopes":["observe","input"]})),
        )
    };
    let first = checked(make_key().await, "createMcpKey", 201).await;
    let key = first["key"].as_str().unwrap().to_owned();
    let first_id = first["entry"]["id"].as_str().unwrap();
    assert!(key.starts_with("kvp_"));
    checked(enable(true).await, "putMcp", 200).await;
    let state = checked(s.call("GET", "/api/mcp", &c, None).await, "getMcp", 200).await;
    assert_eq!(state["keys"][0]["hint"], key[key.len() - 4..]);
    assert!(!state.to_string().contains(&key));
    let shot = |key: &str| {
        s.client
            .get(format!("{}/api/mcp/v1/screenshot", s.url))
            .bearer_auth(key)
            .send()
    };
    let r = shot(&key).await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers()["content-type"], "image/jpeg");
    let b = r.bytes().await.unwrap();
    assert!(b.starts_with(&[255, 216]) && b.ends_with(&[255, 217]));
    let hid = |msg: Value| {
        s.client
            .post(format!("{}/api/mcp/v1/hid", s.url))
            .bearer_auth(&key)
            .json(&msg)
            .send()
    };
    assert_eq!(
        hid(json!({"t":"key","code":"KeyA","down":true}))
            .await
            .unwrap()
            .status(),
        204
    );
    assert_eq!(
        checked(hid(json!({"t":"ping"})).await.unwrap(), "mcpHid", 400).await["error"],
        "bad_message"
    );
    assert_eq!(
        checked(hid(json!({"t":"key"})).await.unwrap(), "mcpHid", 400).await["error"],
        "bad_message"
    );
    assert_eq!(shot("wrong").await.unwrap().status(), 401);
    assert_eq!(
        checked(enable(false).await, "putMcp", 200).await["enabled"],
        false
    );
    assert_eq!(shot(&key).await.unwrap().status(), 403);
    checked(enable(true).await, "putMcp", 200).await;
    let second = checked(make_key().await, "createMcpKey", 201).await;
    let next = second["key"].as_str().unwrap();
    let next_id = second["entry"]["id"].as_str().unwrap();
    assert_ne!(key, next);
    assert_eq!(shot(&key).await.unwrap().status(), 200);
    assert_eq!(shot(next).await.unwrap().status(), 200);
    assert_eq!(
        s.call("DELETE", &format!("/api/mcp/keys/{first_id}"), &c, None)
            .await
            .status(),
        204
    );
    assert_eq!(shot(&key).await.unwrap().status(), 401);
    assert_eq!(
        s.call("DELETE", &format!("/api/mcp/keys/{next_id}"), &c, None)
            .await
            .status(),
        204
    );
    assert_eq!(shot(next).await.unwrap().status(), 403);
}

#[tokio::test]
async fn update_jobs_and_checksums() {
    let s = Server::new().await;
    let c = s.login("admin", "admin").await;
    let settings = json!({"channel":"preview","server":"https://my.example/kvm"});
    assert_eq!(
        checked(
            s.call("PUT", "/api/update/settings", &c, Some(settings.clone()))
                .await,
            "putUpdateSettings",
            200
        )
        .await,
        settings
    );
    let set = checked(
        s.call("GET", "/api/update", &c, None).await,
        "getUpdate",
        200,
    )
    .await;
    assert_eq!(set["channel"], "preview");
    assert_eq!(set["server"], settings["server"]);
    let data = b"kvmpanel-app-0.1.1.tar";
    let up = |sha: &str| {
        s.client
            .put(format!("{}/api/update/offline", s.url))
            .header("cookie", &c)
            .header("x-sha256", sha)
            .header("content-type", "application/octet-stream")
            .body(&data[..])
            .send()
    };
    assert_eq!(
        checked(up("").await.unwrap(), "uploadUpdate", 400).await["detail"]["path"],
        "/x-sha256"
    );
    assert_eq!(
        checked(up(&"0".repeat(64)).await.unwrap(), "uploadUpdate", 422).await["error"],
        "checksum_mismatch"
    );
    checked(
        up(&hex::encode(Sha256::digest(data))).await.unwrap(),
        "uploadUpdate",
        202,
    )
    .await;
    assert_eq!(
        checked(
            s.call("POST", "/api/update/check", &c, None).await,
            "checkUpdate",
            200
        )
        .await["latest"],
        "0.1.1"
    );
    let job = checked(
        s.call("POST", "/api/update/apply", &c, None).await,
        "applyUpdate",
        202,
    )
    .await;
    let id = job["job"]["id"].as_str().unwrap();
    let poll = checked(
        s.call("GET", &format!("/api/update/jobs/{id}"), &c, None)
            .await,
        "getUpdateJob",
        200,
    )
    .await;
    assert_eq!(poll["job"]["id"], id);
    checked(
        s.call("GET", "/api/update/jobs/nope", &c, None).await,
        "getUpdateJob",
        404,
    )
    .await;
}

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
async fn socket(s: &Server, path: &str, c: &str) -> Socket {
    let mut req = format!("ws://{}{path}", s.url.trim_start_matches("http://"))
        .into_client_request()
        .unwrap();
    if !c.is_empty() {
        req.headers_mut().insert("cookie", c.parse().unwrap());
    }
    connect_async(req).await.unwrap().0
}
async fn closed(ws: &mut Socket) -> u16 {
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(msg) = ws.next().await {
            if let Message::Close(Some(frame)) = msg.unwrap() {
                return frame.code.into();
            }
        }
        panic!("socket ended without close frame");
    })
    .await
    .unwrap()
}
async fn limited(s: &Server, c: &str) -> String {
    assert_eq!(s.call("POST","/api/users",c,Some(json!({"name":"viewer","role":"user","password":"abcd1234","permissions":["console"],"view_only":false}))).await.status(),201);
    s.login("viewer", "abcd1234").await
}
async fn until(ws: &mut Socket, needle: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut text = Vec::new();
        while let Some(msg) = ws.next().await {
            if let Message::Binary(bytes) = msg.unwrap() {
                text.extend(bytes);
                if String::from_utf8_lossy(&text).contains(needle) {
                    return;
                }
            }
        }
        panic!("missing {needle}");
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn pty_runs_shell_and_resizes() {
    let s = Server::new().await;
    let c = s.login("admin", "admin").await;
    let mut ws = socket(&s, "/api/terminal?kind=local", &c).await;
    until(&mut ws, "root@kvmpanel:~# ").await;
    ws.send(Message::Text(
        json!({"t":"resize","cols":120,"rows":40})
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    ws.send(Message::Binary(b"stty size\r".as_slice().into()))
        .await
        .unwrap();
    until(&mut ws, "40 120").await;
    ws.send(Message::Binary(b"ls\r".as_slice().into()))
        .await
        .unwrap();
    until(&mut ws, "images").await;
    ws.send(Message::Binary(b"\r".as_slice().into()))
        .await
        .unwrap();
    until(&mut ws, "root@kvmpanel:~# ").await;
    ws.close(None).await.unwrap();
    let viewer = limited(&s, &c).await;
    for (kind, cookie, code) in [
        ("serial", c.as_str(), 4404),
        ("local", viewer.as_str(), 4403),
        ("local", "", 4401),
        ("unknown", c.as_str(), 4400),
    ] {
        assert_eq!(
            closed(&mut socket(&s, &format!("/api/terminal?kind={kind}"), cookie).await).await,
            code
        );
    }
}
#[tokio::test]
async fn scripts_run_with_combined_output_and_permissions() {
    let s = Server::new().await;
    let c = s.login("admin", "admin").await;
    let put = |name: &str, body: &'static str| {
        s.client
            .put(format!("{}/api/scripts/{name}", s.url))
            .header("cookie", &c)
            .header("content-type", "text/plain")
            .body(body)
            .send()
    };
    assert_eq!(
        checked(
            put("hello.sh", "echo hello\n").await.unwrap(),
            "putScript",
            201
        )
        .await,
        json!({"name":"hello.sh","size":11})
    );
    let list = checked(
        s.call("GET", "/api/scripts", &c, None).await,
        "listScripts",
        200,
    )
    .await;
    assert_eq!(list, json!([{"name":"hello.sh","size":11}]));
    let run = || s.call("POST", "/api/scripts/hello.sh/run", &c, None);
    assert_eq!(
        checked(run().await, "runScript", 200).await,
        json!({"exit_code":0,"output":"hello\n"})
    );
    put("hello.sh", "echo a; echo b >&2; exit 7\n")
        .await
        .unwrap();
    assert_eq!(
        checked(run().await, "runScript", 200).await,
        json!({"exit_code":7,"output":"a\nb\n"})
    );
    assert_eq!(
        s.call("DELETE", "/api/scripts/hello.sh", &c, None)
            .await
            .status(),
        204
    );
    checked(run().await, "runScript", 404).await;
    assert_eq!(
        checked(put("..%2Fx", "x").await.unwrap(), "putScript", 400).await["error"],
        "bad_name"
    );
    let user = limited(&s, &c).await;
    assert_eq!(
        s.call("GET", "/api/scripts", &user, None).await.status(),
        403
    );
}

#[tokio::test]
async fn terminal_revocation_while_idle() {
    let s = Server::new().await;
    let c = s.login("admin", "admin").await;
    let u = limited(&s, &c).await;
    let perms = |p: Value| {
        s.call(
            "PATCH",
            "/api/users/viewer",
            &c,
            Some(json!({"permissions":p})),
        )
    };
    assert_eq!(perms(json!(["terminal"])).await.status(), 200);
    let mut ws = socket(&s, "/api/terminal", &u).await;
    until(&mut ws, "kvmpanel").await;
    assert_eq!(perms(json!(["console"])).await.status(), 200);
    assert_eq!(closed(&mut ws).await, 4403);
    for (action, code) in [("disconnect_sessions", 4409), ("restart_kvm", 1012)] {
        let mut ws = socket(&s, "/api/terminal", &c).await;
        until(&mut ws, "kvmpanel").await;
        assert_eq!(
            s.call(
                "POST",
                "/api/device/action",
                &c,
                Some(json!({"action":action}))
            )
            .await
            .status(),
            202
        );
        assert_eq!(closed(&mut ws).await, code);
    }
    let mut ws = socket(&s, "/api/terminal", &c).await;
    until(&mut ws, "kvmpanel").await;
    assert_eq!(
        s.call(
            "POST",
            "/api/users/me/password",
            &c,
            Some(json!({"old":"admin","new":"n3w-pass"}))
        )
        .await
        .status(),
        204
    );
    assert_eq!(closed(&mut ws).await, 4401);
}
