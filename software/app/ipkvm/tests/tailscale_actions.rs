mod common;
use common::Server;
use serde_json::Value;

#[tokio::test]
async fn reports_failed_action_and_clears_it_when_next_starts() {
    let s = Server::new().await;
    let auth = s.login("admin", "admin").await;
    for act in ["uninstall", "up", "install"] {
        let res = s
            .call("POST", &format!("/api/tailscale/{act}"), &auth, None)
            .await;
        assert_eq!(res.status(), 202);
        let status = s
            .call("GET", "/api/tailscale", &auth, None)
            .await
            .json::<Value>()
            .await
            .unwrap();
        kvmd::validate("getTailscale/200", &status).unwrap();
        if act == "up" {
            assert!(status["error"].as_str().unwrap().contains("not installed"));
        } else {
            assert!(status.get("error").is_none());
        }
    }
}
