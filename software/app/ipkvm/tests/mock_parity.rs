mod common;
use common::{cookie, Server};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::{io::AsyncWriteExt, net::TcpStream};

#[tokio::test]
async fn first_login_returns_the_same_public_admin_as_me() {
    let s = Server::new().await;
    let response = s
        .call(
            "POST",
            "/api/auth/login",
            "",
            Some(json!({"username":"admin","password":"admin"})),
        )
        .await;
    assert_eq!(response.status(), 200);
    let session = cookie(&response);
    let login = response.json::<Value>().await.unwrap();
    kvmd::validate("login/200", &login).unwrap();
    let user = &login["user"];
    assert_eq!(user["name"], "admin");
    assert_eq!(user["role"], "admin");
    assert_eq!(user["view_only"], false);
    assert_eq!(user["disabled"], false);
    assert_eq!(user["must_change_password"], true);
    let me = s
        .call("GET", "/api/auth/me", &session, None)
        .await
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(me, login);
}

#[tokio::test(start_paused = true)]
async fn first_two_failures_have_no_intentional_delay() {
    let s = Server::new().await;
    // Keep Tokio's simulated clock stationary while loopback I/O is pending.
    // The wall-clock deadline releases it if a regression introduces a sleep.
    let (release, hold) = std::sync::mpsc::channel::<()>();
    let (ready, started) = tokio::sync::oneshot::channel();
    let guard = tokio::task::spawn_blocking(move || {
        ready.send(()).unwrap();
        let _ = hold.recv_timeout(Duration::from_secs(5));
    });
    started.await.unwrap();
    let before = tokio::time::Instant::now();
    for _ in 0..2 {
        let r = s
            .call(
                "POST",
                "/api/auth/login",
                "",
                Some(json!({"username":"admin","password":"wrong"})),
            )
            .await;
        assert_eq!(r.status(), 401);
        assert_eq!(before.elapsed(), Duration::ZERO);
    }
    drop(release);
    guard.await.unwrap();
}

#[tokio::test]
async fn patching_one_leaf_preserves_the_entire_rest_of_the_configuration() {
    let s = Server::new().await;
    let c = s.login("admin", "admin").await;
    let mut expected = s
        .call("GET", "/api/config", &c, None)
        .await
        .json::<Value>()
        .await
        .unwrap();
    for (section, key, value) in [
        ("web", "title", json!("My panel")),
        ("panel", "brightness", json!(30)),
    ] {
        expected[section][key] = value.clone();
        let res = s
            .call(
                "PATCH",
                "/api/config",
                &c,
                Some(json!({section:{key:value}})),
            )
            .await;
        assert_eq!(res.status(), 200);
        assert_eq!(res.json::<Value>().await.unwrap(), expected);
    }
}

async fn sd_free(s: &Server, session: &str) -> u64 {
    let storage = s
        .call("GET", "/api/storage", session, None)
        .await
        .json::<Value>()
        .await
        .unwrap();
    storage
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["root"] == "sd")
        .unwrap()["free"]
        .as_u64()
        .unwrap()
}
#[tokio::test]
async fn interrupted_first_upload_never_creates_a_listing_entry() {
    let s = Server::new().await;
    let c = s.login("admin", "admin").await;
    let initial = sd_free(&s, &c).await;
    let mut upload = TcpStream::connect(s.url.strip_prefix("http://").unwrap())
        .await
        .unwrap();
    let head=format!("PUT /api/files/upload?root=sd&path=/images/partial.bin HTTP/1.1\r\nHost: localhost\r\nCookie: {c}\r\nContent-Type: application/octet-stream\r\nContent-Length: 200000\r\n\r\n");
    upload.write_all(head.as_bytes()).await.unwrap();
    upload.write_all(&vec![1; 100000]).await.unwrap();
    // Wait for the backend to consume bytes before interrupting the sender.
    tokio::time::timeout(Duration::from_secs(5), async {
        while sd_free(&s, &c).await == initial {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    drop(upload);
    tokio::time::timeout(Duration::from_secs(5), async {
        while sd_free(&s, &c).await != initial {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let list = s
        .call("GET", "/api/files?root=sd&path=/images", &c, None)
        .await
        .json::<Value>()
        .await
        .unwrap();
    assert!(!list["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["name"] == "partial.bin"));
}

#[tokio::test]
async fn mounted_writable_disk_remains_writable_in_media_status() {
    let s = Server::new().await;
    let c = s.login("admin", "admin").await;
    let disk = json!({"root":"sd","path":"/backup/pc-2026-10.img","mode":"disk","readonly":false});
    assert_eq!(
        s.call("POST", "/api/media/mount", &c, Some(disk.clone()))
            .await
            .status(),
        200
    );
    let status = s
        .call("GET", "/api/media", &c, None)
        .await
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(status["mounted"]["path"], disk["path"]);
    assert_eq!(status["readonly"], false);
}
