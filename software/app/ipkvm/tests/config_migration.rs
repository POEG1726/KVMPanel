mod common;
use common::Server;
use serde_json::{json, Value};
use std::fs;

#[tokio::test]
async fn old_files_boot_with_valid_values_and_section_defaults() {
    let clean = Server::new().await;
    let defaults = (*clean.app.config()).clone();
    for (old, expected_brightness) in [
        (json!({"panel":{"brightness":36},"obsolete":{}}), 36),
        (json!({"panel":{"brightness":36,"removed_field":true}}), 70),
        (json!({"panel":{"brightness":"old"}}), 70),
        (json!({"panel":null}), 70),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut config = old;
        config["web"] = json!({"title":"My KVM"});
        fs::write(dir.path().join("config.json"), config.to_string()).unwrap();
        fs::write(dir.path().join("usb-applied.json"), r#"{"net":"yes"}"#).unwrap();
        let s = Server::from_dir(dir).await;
        let cookie = s.login("admin", "admin").await;
        let actual = s
            .call("GET", "/api/config", &cookie, None)
            .await
            .json::<Value>()
            .await
            .unwrap();
        let mut expected = defaults.clone();
        expected["panel"]["brightness"] = expected_brightness.into();
        expected["web"]["title"] = "My KVM".into();
        assert_eq!(actual, expected);
        let disk: Value =
            serde_json::from_slice(&fs::read(s.dir.path().join("config.json")).unwrap()).unwrap();
        assert_eq!(disk, expected);
        let usb = s
            .call("GET", "/api/usb/applied", &cookie, None)
            .await
            .json::<Value>()
            .await
            .unwrap();
        assert_eq!(usb["usb"], defaults["usb"]);
    }
}
