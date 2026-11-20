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

#[tokio::test]
async fn immutable_probe_checks_the_serving_candidate_with_or_without_web() {
    use std::os::unix::fs::symlink;
    use tokio::{
        io::{AsyncBufReadExt, BufReader},
        net::{TcpListener, UnixStream},
        process::Command,
        time::{sleep, timeout, Duration},
    };
    let base = env!("CARGO_BIN_EXE_kvmd");
    let temp = tempfile::tempdir().unwrap();
    let apps = temp.path().join("apps");
    // A separate file/inode models the update's executable and lets SO_PEERCRED
    // distinguish the serving candidate from the verifier in the base image.
    let staged = apps.join("release-trial");
    fs::create_dir_all(&staged).unwrap();
    let binary = staged.join("kvmd");
    fs::copy(base, &binary).unwrap();
    symlink("release-trial", apps.join("current")).unwrap();
    let panel = temp.path().join("panel.sock");
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    fs::write(
        temp.path().join("system.json"),
        serde_json::to_vec(&json!({"network":{
            "bind":"127.0.0.1","http_port":address.port(),"https":{"enabled":true,"port":443}
        }}))
        .unwrap(),
    )
    .unwrap();

    let mut child = Command::new(&binary)
        .arg("--sim")
        .arg("--panel")
        .arg(&panel)
        .arg("--listen")
        .arg(address.to_string())
        .arg("--data-dir")
        .arg(temp.path())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let outcome = async {
        let client = reqwest::Client::new();
        let url = format!("http://{address}");
        timeout(Duration::from_secs(10), async {
            while client
                .get(format!("{url}/api/health"))
                .send()
                .await
                .is_err()
            {
                sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        for enabled in [true, false] {
            if !enabled {
                let mut stream = UnixStream::connect(&panel).await.unwrap();
                use tokio::io::AsyncWriteExt;
                stream.write_all(b"{\"id\":2,\"t\":\"patch_config\",\"patch\":{\"kvm\":{\"enabled\":false}}}\n").await.unwrap();
                let mut lines = BufReader::new(stream).lines();
                timeout(Duration::from_secs(3), async {
                    loop {
                        let reply: serde_json::Value =
                            serde_json::from_str(&lines.next_line().await.unwrap().unwrap())
                                .unwrap();
                        if reply["id"] == 2 && reply["ok"] == true {
                            break;
                        }
                    }
                })
                .await
                .unwrap();
            }
            fs::write(
                apps.join("pending.json"),
                br#"{"candidate":"release-trial","previous":null,"job":"trial"}"#,
            )
            .unwrap();
            let status = timeout(
                Duration::from_secs(6),
                Command::new(base)
                    .arg("--verify-update")
                    .arg("--data-dir")
                    .arg(temp.path())
                    .arg("--panel")
                    .arg(&panel)
                    .kill_on_drop(true)
                    .status(),
            )
            .await
            .unwrap()
            .unwrap();
            assert!(status.success());
            let res: serde_json::Value =
                serde_json::from_slice(&fs::read(apps.join("last-update.json")).unwrap()).unwrap();
            assert_eq!(res, json!({"job":"trial","healthy":true}));
        }
    };
    outcome.await;
    child.kill().await.unwrap();
}

mod common;
#[tokio::test]
async fn new_update_requests_are_busy_during_health_verification() {
    use sha2::{Digest, Sha256};
    let s = common::Server::new().await;
    let a = s.login("admin", "admin").await;
    let apps = s.dir.path().join("apps");
    fs::create_dir_all(&apps).unwrap();
    fs::write(
        apps.join("pending.json"),
        br#"{"candidate":"release-abc","previous":null,"job":"old"}"#,
    )
    .unwrap();
    let online = s.call("POST", "/api/update/apply", &a, None).await;
    let offline = s
        .client
        .put(format!("{}/api/update/offline", s.url))
        .header("cookie", &a)
        .header("x-sha256", hex::encode(Sha256::digest(b"test")))
        .body("test")
        .send()
        .await
        .unwrap();
    for response in [online, offline] {
        assert_eq!(response.status(), 409);
        assert_eq!(
            response.json::<serde_json::Value>().await.unwrap(),
            json!({"error":"busy"})
        );
    }
}
