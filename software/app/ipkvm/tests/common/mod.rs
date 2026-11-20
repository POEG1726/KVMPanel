#![allow(dead_code)]
use kvmd::{router, App};
use reqwest::{Client, Method, Response};
use serde_json::{json, Value};

pub struct Server {
    pub app: App,
    pub url: String,
    pub client: Client,
    pub dir: tempfile::TempDir,
    server: tokio::task::JoinHandle<()>,
}
impl Server {
    pub async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let app = App::new(dir.path(), true).unwrap();
        let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = tcp.local_addr().unwrap();
        let routes = router(app.clone(), &dir.path().join("web"));
        let server = tokio::spawn(async move {
            axum::serve(
                tcp,
                routes.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap()
        });
        Self {
            app,
            url: format!("http://{addr}"),
            client: Client::new(),
            dir,
            server,
        }
    }
    pub async fn login(&self, name: &str, password: &str) -> String {
        let r = self
            .call(
                "POST",
                "/api/auth/login",
                "",
                Some(json!({"username":name,"password":password})),
            )
            .await;
        assert_eq!(r.status(), 200);
        cookie(&r)
    }
    pub async fn call(
        &self,
        method: &str,
        path: &str,
        cookie: &str,
        body: Option<Value>,
    ) -> Response {
        let mut req = self.client.request(
            Method::from_bytes(method.as_bytes()).unwrap(),
            format!("{}{path}", self.url),
        );
        if !cookie.is_empty() {
            req = req.header("cookie", cookie)
        }
        if let Some(v) = body {
            req = req.json(&v)
        }
        req.send().await.unwrap()
    }
}
pub fn cookie(r: &Response) -> String {
    r.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}
impl Drop for Server {
    fn drop(&mut self) {
        self.server.abort()
    }
}
