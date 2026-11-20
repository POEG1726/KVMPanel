mod common;
use serde_json::json;
use std::{path::Path, process::Command};

#[test]
fn every_usb_budget_matches_the_shipped_script() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../os/buildroot/board/kvmpanel/rootfs-overlay/usr/sbin/kvmpanel-gadget");
    let fields = [
        ("keyboard", "KEYBOARD"),
        ("mouse_rel", "REL_MOUSE"),
        ("mouse_abs", "ABS_MOUSE"),
        ("mouse_merge", "MERGE_MICE"),
        ("storage", "STORAGE"),
        ("net", "NETWORK"),
        ("acm", "ACM"),
        ("telemetry", "TELEMETRY"),
    ];
    for mask in 0..256 {
        let mut usb = json!({"net_mode":"ncm"});
        let mut sh = Command::new("sh");
        sh.arg(&script).arg("budget");
        for (bit, (field, key)) in fields.iter().enumerate() {
            let on = mask & (1 << bit) != 0;
            usb[*field] = on.into();
            sh.env(
                key,
                match (*key, on) {
                    ("NETWORK", true) => "ncm",
                    ("NETWORK", false) => "off",
                    (_, true) => "1",
                    (_, false) => "0",
                },
            );
        }
        let budget = kvmd::usb_budget(&usb);
        let output = sh.output().unwrap();
        assert_eq!(output.status.success(), budget["fits"] == true, "{usb}");
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            format!(
                "USB0 endpoints: IN {}/5, OUT {}/5",
                budget["in"], budget["out"]
            )
        );
    }
}

#[tokio::test]
async fn cannot_remove_storage_while_an_image_is_in_use() {
    let s = common::Server::new().await;
    let a = s.login("admin", "admin").await;
    let mounted = s
        .call(
            "POST",
            "/api/media/mount",
            &a,
            Some(json!({"root":"sd","path":"/images/ubuntu-24.04.iso","mode":"cdrom"})),
        )
        .await;
    assert!(mounted.status().is_success());
    s.app
        .patch_config(&json!({"usb":{"storage":false}}), None)
        .unwrap();
    let r = s.call("POST", "/api/usb/apply", &a, None).await;
    assert_eq!(r.status(), 409);
    assert_eq!(
        r.json::<serde_json::Value>().await.unwrap()["error"],
        "in_use"
    );
    assert_eq!(s.app.storage.status()["mode"], "cdrom");
    s.app.storage.eject().unwrap();
    s.app.apply_usb().unwrap();
}
