mod common;
use common::Server;
use kvmd::panel;
use serde_json::{json, Value};
use std::{collections::VecDeque, sync::atomic::Ordering, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines},
    net::{
        unix::{OwnedReadHalf, OwnedWriteHalf},
        UnixStream,
    },
    time::timeout,
};

struct Panel {
    lines: Lines<BufReader<OwnedReadHalf>>,
    write: OwnedWriteHalf,
    queue: VecDeque<Value>,
    task: tokio::task::JoinHandle<std::io::Result<()>>,
}
impl Panel {
    async fn new(s: &Server) -> Self {
        let path = s.dir.path().join("panel.sock");
        let addr = path.display().to_string();
        let app = s.app.clone();
        let task = tokio::spawn(async move { panel::serve(app, &addr).await });
        let stream = timeout(Duration::from_secs(3), async {
            loop {
                if let Ok(stream) = UnixStream::connect(&path).await {
                    break stream;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let (read, write) = stream.into_split();
        Self {
            lines: BufReader::new(read).lines(),
            write,
            task,
            queue: Default::default(),
        }
    }
    async fn read(&mut self) -> Option<Value> {
        let line = self.lines.next_line().await.unwrap()?;
        let v: Value = serde_json::from_str(&line).unwrap();
        kvmd::validate("ToPanel", &v).unwrap();
        Some(v)
    }
    async fn next(&mut self, pred: impl Fn(&Value) -> bool) -> Value {
        if let Some(i) = self.queue.iter().position(&pred) {
            return self.queue.remove(i).unwrap();
        }
        timeout(Duration::from_secs(4), async {
            loop {
                let v = self.read().await.expect("panel unexpectedly closed");
                if pred(&v) {
                    return v;
                }
                self.queue.push_back(v);
            }
        })
        .await
        .unwrap()
    }
    async fn send(&mut self, v: Value) {
        self.write
            .write_all(format!("{v}\n").as_bytes())
            .await
            .unwrap();
    }
    async fn request(&mut self, v: Value) -> Value {
        let id = v["id"].clone();
        self.send(v).await;
        self.next(|m| m["t"] == "result" && m["id"] == id).await
    }
}
impl Drop for Panel {
    fn drop(&mut self) {
        self.task.abort();
    }
}
#[tokio::test]
async fn greetings_metrics_filter_threshold_and_pc_lifetime() {
    let s = Server::new().await;
    let mut p = Panel::new(&s).await;
    assert_eq!(
        p.read().await.unwrap(),
        json!({"t":"hello","version":"0.1.0"})
    );
    let config = p.read().await.unwrap();
    assert_eq!(config["t"], "config");
    assert_eq!(config["config"]["panel"]["language"], "en");
    let status = p.read().await.unwrap();
    assert_eq!(status["eth"]["mbps"], 1000);
    assert_eq!(status["kvm"]["sessions"], 0);
    assert_eq!(status["lux"], 212);
    let first = p.next(|v| v["t"] == "telemetry").await;
    let metrics = first["metrics"].as_array().unwrap();
    assert_eq!(
        metrics
            .iter()
            .filter(|m| m["id"].as_str().unwrap().starts_with("cpu.core."))
            .count(),
        16
    );
    let by_id = |id: &str| metrics.iter().find(|m| m["id"] == id).unwrap();
    let dims: Vec<_> = metrics
        .iter()
        .filter_map(|m| m["id"].as_str()?.strip_prefix("mem.dimm."))
        .collect();
    assert_eq!(dims, ["a2", "b2"]);
    assert!(by_id("mem.dimm.a2")["value"].is_number());
    assert_eq!(by_id("ssd.0.temp")["label"], "SSD 2T");
    let temp = by_id("cpu.tctl");
    assert_eq!(temp["unit"], "°C");
    assert_eq!(temp["min"].as_f64(), Some(30.));
    assert_eq!(temp["max"].as_f64(), Some(95.));
    assert_eq!(temp["warn"].as_f64(), Some(85.));
    assert_eq!(temp["group"], "cpu");
    let result=p.request(json!({"t":"patch_config","id":1,"patch":{"panel":{"hidden_metrics":["cpu.bclk"],"thresholds":{"gpu.temp":70,"mem.*":53,"mem.dimm.*":55,"mem.dimm.a2":57}}}})).await;
    assert_eq!(result["ok"], true);
    let next = p
        .next(|v| {
            v["t"] == "telemetry"
                && !v["metrics"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|m| m["id"] == "cpu.bclk")
        })
        .await;
    let metric = |id: &str| {
        next["metrics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == id)
            .unwrap()
    };
    for (id, warn) in [
        ("gpu.temp", 70.),
        ("mem.dimm.b2", 55.),
        ("mem.dimm.a2", 57.),
        ("cpu.tctl", 85.),
    ] {
        assert_eq!(metric(id)["warn"].as_f64(), Some(warn));
    }
    assert!(metric("gpu.power").get("warn").is_none());
    s.app
        .console
        .sim_pc_connected
        .store(false, Ordering::Relaxed);
    let down = p.next(|v| v["t"] == "pc").await;
    assert_eq!(down["connected"], false);
    assert!(down["last_seen_ts"].is_number());
    assert_eq!(
        p.next(|m| m["t"] == "status" && m["pc"]["state"] == "off")
            .await["pc"]["state"],
        "off"
    );
    assert!(timeout(Duration::from_millis(1500), async {
        while let Some(v) = p.read().await {
            if v["t"] == "telemetry" {
                break;
            }
        }
    })
    .await
    .is_err());
    s.app
        .console
        .sim_pc_connected
        .store(true, Ordering::Relaxed);
    assert_eq!(p.next(|v| v["t"] == "pc").await["connected"], true);
    p.next(|v| v["t"] == "telemetry").await;
}

#[tokio::test]
async fn shared_config_bad_lines_actions_and_restart() {
    let s = Server::new().await;
    let c = s.login("admin", "admin").await;
    let mut p = Panel::new(&s).await;
    let cfg = p.next(|m| m["t"] == "config").await["config"].clone();
    assert_eq!(
        p.request(json!({"t":"get_config","id":0})).await["data"],
        cfg
    );
    let bad = p
        .request(json!({"t":"patch_config","id":7,"patch":{"usb":{"net":true}}}))
        .await;
    assert_eq!(
        bad,
        json!({"t":"result","id":7,"ok":false,"error":"endpoints_full","detail":{"in_missing":1,"out_missing":0}})
    );
    let invalid = p
        .request(json!({"t":"patch_config","id":17,"patch":{"panel":{"brightness":130}}}))
        .await;
    assert_eq!(invalid["error"], "invalid_config");
    let changed = p
        .request(json!({"t":"patch_config","id":8,"patch":{"panel":{"language":"zh"}}}))
        .await;
    assert_eq!(changed["ok"], true);
    assert_eq!(changed["data"]["panel"]["language"], "zh");
    p.next(|v| v["t"] == "config" && v["config"]["panel"]["language"] == "zh")
        .await;
    assert_eq!(
        s.call(
            "PATCH",
            "/api/config",
            &c,
            Some(json!({"panel":{"brightness":33}}))
        )
        .await
        .status(),
        200
    );
    p.next(|v| v["t"] == "config" && v["config"]["panel"]["brightness"] == 33)
        .await;
    p.write.write_all(b"{not json\n").await.unwrap();
    assert_eq!(
        p.next(|v| v["t"] == "result").await,
        json!({"t":"result","ok":false,"error":"bad_message"})
    );
    assert_eq!(
        p.request(json!({"t":"nope","id":2})).await,
        json!({"t":"result","id":2,"ok":false,"error":"bad_message"})
    );
    let info = p.request(json!({"t":"device_info","id":3})).await;
    assert_eq!(info["ok"], true);
    assert_eq!(info["data"]["hostname"], "kvmpanel");
    assert_eq!(info["data"]["lux"], 212);
    for action in [
        "usb_apply",
        "reset_hid",
        "disconnect_sessions",
        "reboot",
        "shutdown",
        "eject_media",
        "restart_kvm",
    ] {
        assert_eq!(
            p.request(json!({"t":"action","id":4,"action":action}))
                .await,
            json!({"t":"result","id":4,"ok":true})
        );
    }
    p.next(|v| v["t"] == "kvm_restarting").await;
    assert!(timeout(Duration::from_secs(1), p.read())
        .await
        .unwrap()
        .is_none());
    let mut again = UnixStream::connect(s.dir.path().join("panel.sock"))
        .await
        .unwrap();
    let mut hello = String::new();
    BufReader::new(&mut again)
        .read_line(&mut hello)
        .await
        .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&hello).unwrap()["t"], "hello");
}

#[tokio::test]
async fn refuses_public_trusted_tcp_socket() {
    let s = Server::new().await;
    assert_eq!(
        panel::serve(s.app.clone(), "0.0.0.0:0")
            .await
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::InvalidInput
    );
}

#[tokio::test]
async fn each_committed_patch_reaches_panel_in_order() {
    let s = Server::new().await;
    let mut p = Panel::new(&s).await;
    p.next(|m| m["t"] == "config").await;
    // Back-to-back writers complete before the single-thread test runtime polls the display.
    for brightness in [18, 34, 72] {
        s.app
            .patch_config(&json!({"panel":{"brightness":brightness}}), None)
            .unwrap();
    }
    for brightness in [18, 34, 72] {
        assert_eq!(
            p.next(|m| m["t"] == "config").await["config"]["panel"]["brightness"],
            brightness
        );
    }
}

#[tokio::test]
async fn a_lagging_panel_can_query_config_and_still_receives_restart() {
    let s = Server::new().await;
    let mut panel = Panel::new(&s).await;
    panel.next(|v| v["t"] == "status").await;
    for _ in 0..512 {
        s.app.emit("status_changed");
    }
    assert_eq!(
        panel.request(json!({"t":"get_config","id":51})).await["data"]["panel"]["brightness"],
        70
    );
    s.app.emit("kvm_restarting");
    for _ in 0..512 {
        s.app.emit("status_changed");
    }
    panel.next(|v| v["t"] == "kvm_restarting").await;
}
