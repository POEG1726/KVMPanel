mod common;
mod console_support;
use common::Server;
use console_support::*;
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn proactive_close_for_all_remote_transports() {
    let s = Server::new().await;
    let admin = s.login("admin", "admin").await;
    let grant = ["console", "terminal"];
    let mut c = user(&s, &admin, "tom", &grant, false).await;
    let paths = ["/api/ws", "/api/stream", "/api/terminal?kind=local"];
    for (path, body, method, want) in [
        (
            "/api/users/me/password",
            json!({"old":"pw123456","new":"new-pass"}),
            "POST",
            4401,
        ),
        ("/api/users/tom", json!({"permissions":[]}), "PATCH", 4403),
        (
            "/api/device/action",
            json!({"action":"disconnect_sessions"}),
            "POST",
            4409,
        ),
        (
            "/api/device/action",
            json!({"action":"restart_kvm"}),
            "POST",
            1012,
        ),
    ] {
        let mut sockets = Vec::new();
        for url in paths {
            sockets.push(open(&s, url, &c).await);
        }
        next_json(&mut sockets[0], |v| v["t"] == "status").await;
        let r = s
            .call(
                method,
                path,
                if want == 4401 { &c } else { &admin },
                Some(body),
            )
            .await;
        assert!(r.status().is_success());
        let mut restarted = false;
        let observed = tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                match receive(&mut sockets[0]).await {
                    Message::Text(t) => {
                        let v: Value = serde_json::from_str(&t).unwrap();
                        restarted |= v["t"] == "kvm_restarting";
                    }
                    Message::Close(Some(f)) => break u16::from(f.code),
                    _ => {}
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(observed, want);
        assert_eq!(restarted, want == 1012);
        for socket in &mut sockets[1..] {
            assert_eq!(code(socket).await, want);
        }
        if want == 4401 {
            c = s.login("tom", "new-pass").await;
        }
        if want == 4403 {
            assert_eq!(
                s.call(
                    "PATCH",
                    "/api/users/tom",
                    &admin,
                    Some(json!({"permissions":grant}))
                )
                .await
                .status(),
                200
            );
        }
    }
}

#[tokio::test]
async fn revoking_terminal_preserves_console_and_logout_closes_it() {
    let s = Server::new().await;
    let a = s.login("admin", "admin").await;
    let u = user(&s, &a, "operator", &["console", "terminal"], false).await;
    let mut con = open(&s, "/api/ws", &u).await;
    let mut term = open(&s, "/api/terminal?kind=local", &u).await;
    next_json(&mut con, |v| v["t"] == "status").await;
    s.call(
        "PATCH",
        "/api/users/operator",
        &a,
        Some(json!({"permissions":["console"]})),
    )
    .await;
    assert_eq!(code(&mut term).await, 4403);
    send(&mut con, json!({"t":"ping"})).await;
    assert_eq!(
        next_json(&mut con, |v| v["t"] == "pong").await,
        json!({"t":"pong"})
    );
    assert_eq!(
        s.call("POST", "/api/auth/logout", &u, None).await.status(),
        204
    );
    assert_eq!(code(&mut con).await, 4401);
}

#[tokio::test]
async fn account_deletion_disabling_and_logout_revoke_every_socket() {
    let s = Server::new().await;
    let admin = s.login("admin", "admin").await;
    for mode in ["delete", "disable", "logout"] {
        let c = user(&s, &admin, mode, &["console", "terminal"], false).await;
        let mut sockets = Vec::new();
        for path in ["/api/ws", "/api/stream", "/api/terminal?kind=local"] {
            sockets.push(open(&s, path, &c).await);
        }
        next_json(&mut sockets[0], |v| v["t"] == "status").await;
        let path = format!("/api/users/{mode}");
        let change = match mode {
            "delete" => s.call("DELETE", &path, &admin, None).await,
            "disable" => {
                s.call("PATCH", &path, &admin, Some(json!({"disabled":true})))
                    .await
            }
            _ => s.call("POST", "/api/auth/logout", &c, None).await,
        };
        assert!(change.status().is_success());
        for socket in &mut sockets {
            assert_eq!(code(socket).await, 4401);
        }
    }
}
