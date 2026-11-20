mod common;
mod console_support;
use common::Server;
use console_support::*;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use tokio::time::{timeout, Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn websocket_status_input_messages_and_usb_apply() {
    let s = Server::new().await;
    let admin = s.login("admin", "admin").await;
    let mut ws = open(&s, "/api/ws", &admin).await;
    let initial = next_json(&mut ws, |m| m["t"] == "status").await;
    assert_eq!(initial["sessions"][0]["user"], "admin");
    assert_eq!(initial["sessions"][0]["ip"], "127.0.0.1");
    assert_eq!(initial["sessions"][0]["view_only"], false);
    send(&mut ws, json!({"t":"key","code":"CapsLock","down":true})).await;
    let changed = next_json(&mut ws, |m| {
        m["t"] == "status" && m["locks"]["caps"] == true
    })
    .await;
    assert_eq!(
        changed["locks"],
        json!({"num":false,"caps":true,"scroll":false})
    );
    for bad in [
        Message::Text(r#"{"t":"key"}"#.into()),
        Message::Text("not json".into()),
        Message::Binary(vec![0].into()),
        Message::Text(r#"{"t":"mouse_abs","x":70000,"y":0,"buttons":0}"#.into()),
        Message::Text(r#"{"t":"ping","extra":true}"#.into()),
        Message::Text(
            json!({"t":"paste","text":"界".repeat(21846)})
                .to_string()
                .into(),
        ),
    ] {
        ws.send(bad).await.unwrap();
        assert_eq!(
            next_json(&mut ws, |m| m["t"] == "error").await,
            json!({"t":"error","error":"bad_message"})
        );
    }
    for valid in [
        json!({"t":"mouse_abs","x":65535,"y":0,"buttons":31}),
        json!({"t":"mouse_rel","dx":-127,"dy":127,"buttons":2}),
        json!({"t":"paste","text":"hello"}),
        json!({"t":"wheel","dy":-1}),
        json!({"t":"shortcut","keys":["ControlLeft","KeyA"]}),
    ] {
        send(&mut ws, valid).await;
    }
    send(&mut ws, json!({"t":"ping"})).await;
    assert_eq!(
        next_json(&mut ws, |m| m["t"] != "status").await,
        json!({"t":"pong"})
    );
    assert_eq!(
        s.call("POST", "/api/usb/apply", &admin, None)
            .await
            .status(),
        202
    );
    assert_eq!(
        next_json(&mut ws, |m| m["t"] == "usb_reenumerated").await,
        json!({"t":"usb_reenumerated"})
    );
}

#[tokio::test]
async fn console_socket_authorization_and_read_only() {
    let s = Server::new().await;
    let admin = s.login("admin", "admin").await;
    let observer = user(&s, &admin, "observer", &["console"], true).await;
    let mut ws = open(&s, "/api/ws", &observer).await;
    assert_eq!(
        next_json(&mut ws, |m| m["t"] == "status").await["sessions"][0]["view_only"],
        true
    );
    send(&mut ws, json!({"t":"key","code":"KeyA","down":true})).await;
    assert_eq!(
        next_json(&mut ws, |m| m["t"] == "error").await,
        json!({"t":"error","error":"forbidden"})
    );
    let denied = user(&s, &admin, "guest", &["panel"], false).await;
    for path in ["/api/ws", "/api/stream"] {
        assert_eq!(code(&mut open(&s, path, "").await).await, 4401);
        assert_eq!(code(&mut open(&s, path, &denied).await).await, 4403);
    }
}

#[tokio::test]
async fn power_and_wake_on_lan_contract() {
    let s = Server::new().await;
    let c = s.login("admin", "admin").await;
    let get_power = || s.call("GET", "/api/power", &c, None);
    assert_eq!(
        checked(get_power().await, "getPower", 200).await,
        json!({"on":true,"hdd":false})
    );
    for (button, ms, on) in [
        ("power", 300, false),
        ("power", 300, true),
        ("reset", 300, true),
        ("power", 5000, false),
        ("power", 5000, false),
    ] {
        assert_eq!(
            s.call(
                "POST",
                "/api/power/press",
                &c,
                Some(json!({"button":button,"duration_ms":ms}))
            )
            .await
            .status(),
            204
        );
        assert_eq!(checked(get_power().await, "getPower", 200).await["on"], on);
    }
    let body = json!({"button":"power","duration_ms":50});
    checked(
        s.call("POST", "/api/power/press", &c, Some(body)).await,
        "pressPower",
        400,
    )
    .await;
    let observer = user(&s, &c, "observer", &["console"], false).await;
    checked(
        s.call(
            "POST",
            "/api/power/press",
            &observer,
            Some(json!({"button":"power","duration_ms":300})),
        )
        .await,
        "pressPower",
        403,
    )
    .await;
    let bad = checked(
        s.call(
            "POST",
            "/api/wol/devices",
            &c,
            Some(json!({"name":"x","mac":"zz"})),
        )
        .await,
        "addWolDevice",
        400,
    )
    .await;
    assert_eq!(bad["error"], "bad_mac");
    let device = json!({"name":"本机","mac":"a8:a1:59:12:34:3c"});
    assert_eq!(
        checked(
            s.call(
                "POST",
                "/api/wol/devices",
                &c,
                Some(json!({"name":"本机","mac":"A8-A1-59-12-34-3C"}))
            )
            .await,
            "addWolDevice",
            201
        )
        .await,
        device
    );
    checked(
        s.call("POST", "/api/wol/devices", &c, Some(device.clone()))
            .await,
        "addWolDevice",
        409,
    )
    .await;
    assert_eq!(
        checked(
            s.call("GET", "/api/wol/devices", &c, None).await,
            "listWolDevices",
            200
        )
        .await,
        json!({"devices":[device]})
    );
    let persisted: Value =
        serde_json::from_slice(&std::fs::read(s.dir.path().join("wol.json")).unwrap()).unwrap();
    assert_eq!(persisted, json!([device]));
    assert_eq!(
        s.call(
            "POST",
            "/api/wol/wake",
            &c,
            Some(json!({"mac":device["mac"]}))
        )
        .await
        .status(),
        204
    );
    assert_eq!(
        checked(
            s.call("POST", "/api/wol/wake", &c, Some(json!({"mac":"nope"})))
                .await,
            "wakeOnLan",
            400
        )
        .await["error"],
        "bad_mac"
    );
    let del = || s.call("DELETE", "/api/wol/devices/a8:a1:59:12:34:3c", &c, None);
    assert_eq!(del().await.status(), 204);
    checked(del().await, "deleteWolDevice", 404).await;
}

fn jpeg(data: &[u8]) {
    assert_eq!(&data[..2], &[0xff, 0xd8]);
    assert_eq!(&data[data.len() - 2..], &[0xff, 0xd9]);
    // Baseline SOF includes the precision and big-endian dimensions.
    let i = data.windows(2).position(|b| b == [0xff, 0xc0]).unwrap();
    assert_eq!(&data[i + 5..i + 9], &[0x04, 0x38, 0x07, 0x80]);
}
#[tokio::test]
async fn snapshot_edid_and_no_signal() {
    let s = Server::new().await;
    let c = s.login("admin", "admin").await;
    let image = s.call("GET", "/api/stream/snapshot.jpg", &c, None).await;
    assert_eq!(image.status(), 200);
    assert_eq!(image.headers()["content-type"], "image/jpeg");
    jpeg(&image.bytes().await.unwrap());
    s.app.console.sim_signal.store(false, Ordering::Relaxed);
    assert_eq!(
        checked(
            s.call("GET", "/api/stream/snapshot.jpg", &c, None).await,
            "getSnapshot",
            503
        )
        .await,
        json!({"error":"no_signal"})
    );
    let initial = checked(
        s.call("GET", "/api/video/edid", &c, None).await,
        "getEdid",
        200,
    )
    .await;
    let bytes = hex::decode(initial["hex"].as_str().unwrap()).unwrap();
    assert_eq!(initial["preset"], "1080p");
    assert_eq!(bytes.len(), 128);
    assert!(bytes.starts_with(&[0, 255, 255, 255, 255, 255, 255, 0]));
    assert_eq!(bytes.into_iter().fold(0, u8::wrapping_add), 0);
    assert_eq!(
        checked(
            s.call("PUT", "/api/video/edid", &c, Some(json!({"hex":"00ff"})))
                .await,
            "putEdid",
            400
        )
        .await["error"],
        "bad_edid"
    );
    let custom = format!("00ffffffffffff00{}", "11".repeat(120));
    assert_eq!(
        checked(
            s.call("PUT", "/api/video/edid", &c, Some(json!({"hex":custom})))
                .await,
            "putEdid",
            200
        )
        .await,
        json!({"preset":"custom","hex":custom})
    );
    assert_eq!(
        checked(
            s.call("POST", "/api/video/edid/reset", &c, None).await,
            "resetEdid",
            200
        )
        .await,
        initial
    );
}

#[tokio::test]
async fn stream_rate_pause_and_resume() {
    let s = Server::new().await;
    let c = s.login("admin", "admin").await;
    assert_eq!(
        s.call(
            "PATCH",
            "/api/config",
            &c,
            Some(json!({"video":{"fps_max":5}}))
        )
        .await
        .status(),
        200
    );
    let mut ws = open(&s, "/api/stream", &c).await;
    let mut count = 0;
    let start = Instant::now();
    while let Ok(Some(Ok(Message::Binary(bytes)))) =
        tokio::time::timeout_at(start + Duration::from_secs(2), ws.next()).await
    {
        jpeg(&bytes);
        count += 1;
    }
    assert!((8..=12).contains(&count), "{count} frames");
    send(&mut ws, json!({"t":"pause"})).await;
    let until = Instant::now() + Duration::from_millis(180);
    while tokio::time::timeout_at(until, ws.next()).await.is_ok() {}
    assert!(timeout(Duration::from_secs(1), ws.next()).await.is_err());
    send(&mut ws, json!({"t":"resume"})).await;
    assert!(matches!(receive(&mut ws).await, Message::Binary(_)));
}
