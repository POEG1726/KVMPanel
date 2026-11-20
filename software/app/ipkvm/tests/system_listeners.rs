use futures_util::StreamExt;
use kvmd::App;
use serde_json::{json, Value};
use std::{
    net::{SocketAddr, TcpListener},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
    process::Command,
    time::{sleep, timeout},
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
};

async fn online(addr: SocketAddr) -> TcpStream {
    timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(tcp) = TcpStream::connect(addr).await {
                return tcp;
            }
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("listener did not start")
}
async fn enable(panel: &mut BufReader<TcpStream>, id: u32) {
    let patch = json!({"t":"patch_config","patch":{"kvm":{"enabled":true}},"id":id});
    panel
        .get_mut()
        .write_all(format!("{patch}\n").as_bytes())
        .await
        .unwrap();
    timeout(Duration::from_secs(3), async {
        loop {
            let mut buf = String::new();
            assert!(panel.read_line(&mut buf).await.unwrap() > 0);
            let v: Value = serde_json::from_str(&buf).unwrap();
            if v["t"] == "result" && v["id"] == id {
                assert_eq!(v["ok"], true);
                assert_eq!(v["data"]["kvm"]["enabled"], true);
                break;
            }
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn panel_restores_the_web_listener_and_disabling_drains_http() {
    let dir = tempfile::tempdir().unwrap();
    let state = App::new(dir.path(), true).unwrap();
    state
        .patch_config(&json!({"kvm":{"enabled":false}}), None)
        .unwrap();
    drop(state);

    let http = TcpListener::bind("127.0.0.1:0").unwrap();
    let panel = TcpListener::bind("127.0.0.1:0").unwrap();
    let web_addr = http.local_addr().unwrap();
    let panel_addr = panel.local_addr().unwrap();
    drop((http, panel));
    let mut child = Command::new(env!("CARGO_BIN_EXE_kvmd"))
        .arg("--sim")
        .arg("--listen")
        .arg(web_addr.to_string())
        .arg("--panel")
        .arg(panel_addr.to_string())
        .arg("--data-dir")
        .arg(dir.path())
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    let mut panel = BufReader::new(online(panel_addr).await);
    let mut line = String::new();
    panel.read_line(&mut line).await.unwrap();
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap()["t"], "hello");
    assert!(TcpStream::connect(web_addr).await.is_err());
    enable(&mut panel, 1).await;
    let mut idle = online(web_addr).await;

    // Exercise a real, idle HTTP/1 keep-alive connection separately from the PATCH.
    idle.write_all(format!("GET /api/health HTTP/1.1\r\nHost: {web_addr}\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut idle = BufReader::new(idle);
    let mut size = None;
    loop {
        line.clear();
        assert!(idle.read_line(&mut line).await.unwrap() > 0);
        if line == "\r\n" {
            break;
        }
        if let Some(n) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            size = Some(n.trim().parse::<usize>().unwrap());
        }
    }
    let mut data = vec![0; size.unwrap()];
    idle.read_exact(&mut data).await.unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&data).unwrap()["ok"], true);

    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let login = client
        .post(format!("http://{web_addr}/api/auth/login"))
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
    let mut request = format!("ws://{web_addr}/api/ws")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("cookie", cookie.parse().unwrap());
    let mut ws = connect_async(request).await.unwrap().0;
    assert_eq!(
        serde_json::from_str::<Value>(&ws.next().await.unwrap().unwrap().into_text().unwrap())
            .unwrap()["t"],
        "status"
    );

    let path = format!("http://{web_addr}/api/config");
    let off = client
        .patch(&path)
        .header("cookie", &cookie)
        .json(&json!({"kvm":{"enabled":false}}))
        .send()
        .await
        .unwrap();
    assert_eq!(off.status(), 200);
    assert_eq!(off.json::<Value>().await.unwrap()["kvm"]["enabled"], false);
    timeout(Duration::from_secs(4), async {
        loop {
            if let Message::Close(Some(close)) = ws.next().await.unwrap().unwrap() {
                assert_eq!(u16::from(close.code), 4409);
                break;
            }
        }
        let mut byte = [0u8; 1];
        assert_eq!(idle.read(&mut byte).await.unwrap(), 0);
        while TcpStream::connect(web_addr).await.is_ok() {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    enable(&mut panel, 2).await;
    drop(online(web_addr).await);
    let cfg = client
        .get(&path)
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(cfg["kvm"]["enabled"], true);
    child.kill().await.unwrap();
}
