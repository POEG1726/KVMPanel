use kvmd::system::System;
use serde_json::json;
use std::fs;

#[test]
fn settings_from_an_older_release_fill_missing_fields() {
    let data = tempfile::tempdir().unwrap();
    fs::write(
        data.path().join("system.json"),
        br#"{"scripts":{"hello":"echo hi"},"obsolete":3}"#,
    )
    .unwrap();
    let system = System::new(data.path()).unwrap();
    assert_eq!(system.network()["http_port"], json!(80));
    assert_eq!(system.network()["https"]["enabled"], json!(false));
}
