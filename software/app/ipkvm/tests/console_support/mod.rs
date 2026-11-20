#![allow(dead_code)]
use crate::common::Server;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::time::{timeout, Duration};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
    MaybeTlsStream, WebSocketStream,
};
pub type Ws = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
pub async fn open(s: &Server, path: &str, cookie: &str) -> Ws {
    let mut req = format!("{}{path}", s.url.replacen("http:", "ws:", 1))
        .into_client_request()
        .unwrap();
    if !cookie.is_empty() {
        req.headers_mut().insert("cookie", cookie.parse().unwrap());
    }
    connect_async(req).await.unwrap().0
}
pub async fn receive(ws: &mut Ws) -> Message {
    timeout(Duration::from_secs(6), ws.next())
        .await
        .expect("websocket timeout")
        .unwrap()
        .unwrap()
}
pub async fn next_json(ws: &mut Ws, predicate: impl Fn(&Value) -> bool) -> Value {
    timeout(Duration::from_secs(5), async {
        loop {
            let m = receive(ws).await;
            let text = m.into_text().unwrap();
            let v: Value = serde_json::from_str(&text).unwrap();
            kvmd::validate("ServerMessage", &v).unwrap();
            if predicate(&v) {
                return v;
            }
        }
    })
    .await
    .expect("matching message")
}
pub async fn send(ws: &mut Ws, v: Value) {
    ws.send(Message::Text(v.to_string().into())).await.unwrap();
}
pub async fn code(ws: &mut Ws) -> u16 {
    timeout(Duration::from_secs(5), async {
        loop {
            if let Message::Close(Some(c)) = receive(ws).await {
                return u16::from(c.code);
            }
        }
    })
    .await
    .unwrap()
}
pub async fn user(
    s: &Server,
    admin: &str,
    name: &str,
    permissions: &[&str],
    view_only: bool,
) -> String {
    let r=s.call("POST","/api/users",admin,Some(json!({"name":name,"password":"pw123456","role":"user","permissions":permissions,"view_only":view_only}))).await;
    assert_eq!(r.status(), 201);
    s.login(name, "pw123456").await
}
pub async fn checked(r: reqwest::Response, op: &str, status: u16) -> Value {
    assert_eq!(r.status(), status);
    let v: Value = r.json().await.unwrap();
    kvmd::validate(&format!("{op}/{status}"), &v).unwrap();
    v
}
