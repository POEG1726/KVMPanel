mod common;
use common::Server;
use reqwest::Response;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Api {
    s: Server,
    cookie: String,
}
impl Api {
    async fn new() -> Self {
        let s = Server::new().await;
        let cookie = s.login("admin", "admin").await;
        Self { s, cookie }
    }
    async fn get(&self, path: &str) -> Response {
        self.s.call("GET", path, &self.cookie, None).await
    }
    async fn post(&self, path: &str, body: Value) -> Response {
        self.s.call("POST", path, &self.cookie, Some(body)).await
    }
    async fn upload(&self, which: &str, path: &str, content: &[u8], sum: Option<&str>) -> Response {
        let mut req = self
            .s
            .client
            .put(format!("{}/api/files/upload", self.s.url))
            .query(&[("root", which), ("path", path)])
            .header("cookie", &self.cookie)
            .header("content-type", "application/octet-stream");
        if let Some(v) = sum {
            req = req.header("x-sha256", v);
        }
        req.body(content.to_vec()).send().await.unwrap()
    }
    async fn list(&self, root: &str, path: &str) -> Response {
        self.s
            .client
            .get(format!("{}/api/files", self.s.url))
            .query(&[("root", root), ("path", path)])
            .header("cookie", &self.cookie)
            .send()
            .await
            .unwrap()
    }
}

async fn check(res: Response, op: &str, code: u16) -> Value {
    assert_eq!(res.status().as_u16(), code, "{op}");
    if code == 204 {
        assert!(res.bytes().await.unwrap().is_empty());
        return Value::Null;
    }
    let body = res.json().await.unwrap();
    kvmd::validate(&format!("{op}/{code}"), &body).unwrap_or_else(|e| panic!("{op} {body} {e:?}"));
    body
}
fn names(entries: &Value) -> Vec<&str> {
    entries["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn roots_paths_and_permissions() {
    let a = Api::new().await;
    let volumes = check(a.get("/api/storage").await, "getStorage", 200).await;
    assert_eq!(
        volumes[0],
        json!({"root":"sd","total":58_000_000_000u64,"free":34_600_000_000u64,"mounted":true})
    );
    let ls = check(a.list("sd", "/images").await, "listFiles", 200).await;
    assert_eq!(
        names(&ls),
        [
            "drivers",
            "bios-F12a.zip",
            "notes.txt",
            "ubuntu-24.04.iso",
            "win11-24h2.iso"
        ]
    );
    for (root, path) in [
        ("sd", "/images/../../etc"),
        ("sd", "../x"),
        ("share", "/a/../b"),
        ("sd", "images"),
    ] {
        assert_eq!(
            check(a.list(root, path).await, "listFiles", 400).await["error"],
            "bad_path"
        );
    }
    assert_eq!(
        names(&check(a.list("system", "/foo/../etc").await, "listFiles", 200).await),
        ["hostname"]
    );
    assert_eq!(
        check(a.list("sd", "/nope").await, "listFiles", 404).await,
        json!({"error":"not_found"})
    );
    let sd = a.s.dir.path().join("volumes/sd");
    std::os::unix::fs::symlink(a.s.dir.path(), sd.join("outside")).unwrap();
    assert_eq!(
        check(a.list("sd", "/outside").await, "listFiles", 400).await["error"],
        "bad_path"
    );
    for (name, perm) in [("filer", "files"), ("viewer", "console")] {
        check(
            a.post(
                "/api/users",
                json!({"name":name,"password":"pw123456","role":"user","permissions":[perm]}),
            )
            .await,
            "createUser",
            201,
        )
        .await;
        let cookie = a.s.login(name, "pw123456").await;
        let res =
            a.s.call("GET", "/api/files?root=system&path=/etc", &cookie, None)
                .await;
        check(res, "listFiles", 403).await;
        let res =
            a.s.call("GET", "/api/files?root=sd&path=/", &cookie, None)
                .await;
        check(res, "listFiles", if perm == "files" { 200 } else { 403 }).await;
    }
}

#[tokio::test]
async fn files_round_trip_and_export() {
    let a = Api::new().await;
    let data = b"hello kvmpanel\n".repeat(1000);
    let sum = format!("{:x}", Sha256::digest(&data));
    let up = check(
        a.upload("sd", "/images/hello.txt", &data, Some(&sum)).await,
        "uploadFile",
        201,
    )
    .await;
    assert_eq!(up["size"], data.len());
    assert_eq!(up["type"], "file");
    let ls = check(a.list("sd", "/images").await, "listFiles", 200).await;
    assert!(ls["entries"].as_array().unwrap().contains(&up));
    let download = a
        .get("/api/files/download?root=sd&path=/images/hello.txt")
        .await;
    assert_eq!(download.status(), 200);
    assert!(download.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .starts_with("attachment;"));
    assert_eq!(download.bytes().await.unwrap().as_ref(), data.as_slice());
    for path in ["/images/bad.bin", "/images/hello.txt"] {
        assert_eq!(
            check(
                a.upload("sd", path, b"abc", Some(&"0".repeat(64))).await,
                "uploadFile",
                422
            )
            .await["error"],
            "checksum_mismatch"
        );
    }
    assert!(
        !names(&check(a.list("sd", "/images").await, "listFiles", 200).await).contains(&"bad.bin")
    );
    assert_eq!(
        fs::read(a.s.dir.path().join("volumes/sd/images/hello.txt")).unwrap(),
        data
    );
    let dir = json!({"root":"sd","path":"/isos"});
    check(
        a.post("/api/files/mkdir", dir.clone()).await,
        "makeDir",
        201,
    )
    .await;
    check(a.post("/api/files/mkdir", dir).await, "makeDir", 409).await;
    check(
        a.post(
            "/api/files/rename",
            json!({"root":"sd","path":"/isos","to":"/iso-store"}),
        )
        .await,
        "renamePath",
        204,
    )
    .await;
    for code in [204, 404] {
        check(
            a.post(
                "/api/files/delete",
                json!({"root":"sd","path":"/iso-store"}),
            )
            .await,
            "deletePath",
            code,
        )
        .await;
    }
    check(
        a.post("/api/share", json!({"exported_to_host":true})).await,
        "setShare",
        200,
    )
    .await;
    assert_eq!(
        check(
            a.upload("share", "/note.txt", b"x", None).await,
            "uploadFile",
            409
        )
        .await["error"],
        "share_busy"
    );
    assert_eq!(
        check(
            a.post("/api/files/mkdir", json!({"root":"share","path":"/d"}))
                .await,
            "makeDir",
            409
        )
        .await["error"],
        "share_busy"
    );
    assert_eq!(
        check(a.get("/api/share").await, "getShare", 200).await,
        json!({"exported_to_host":true})
    );
    check(
        a.post("/api/share", json!({"exported_to_host":false}))
            .await,
        "setShare",
        200,
    )
    .await;
    check(
        a.upload("share", "/note.txt", b"x", None).await,
        "uploadFile",
        201,
    )
    .await;
}

async fn socket_upload(a: &Api, root: &str, path: &str, framing: &str) -> tokio::net::TcpStream {
    let mut sock = tokio::net::TcpStream::connect(a.s.url.strip_prefix("http://").unwrap())
        .await
        .unwrap();
    let headers=format!("PUT /api/files/upload?root={root}&path={path} HTTP/1.1\r\nHost: localhost\r\nCookie: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n{framing}\r\n\r\n",a.cookie);
    sock.write_all(headers.as_bytes()).await.unwrap();
    sock
}
async fn too_large(mut sock: tokio::net::TcpStream) {
    let mut reply = Vec::new();
    tokio::time::timeout(Duration::from_secs(4), sock.read_to_end(&mut reply))
        .await
        .unwrap()
        .unwrap();
    let reply = String::from_utf8(reply).unwrap();
    assert!(reply.starts_with("HTTP/1.1 507"), "{reply}");
    assert!(reply.contains(r#"{"error":"no_space"}"#));
}
#[tokio::test]
async fn partial_uploads_and_space_limits() {
    let a = Api::new().await;
    let sd = a.s.dir.path().join("volumes/sd/images");
    let mut huge = socket_upload(&a, "sd", "/images/huge.img", "Content-Length: 40000000000").await;
    huge.write_all(&[0; 1024]).await.unwrap();
    too_large(huge).await;
    let share = a.s.dir.path().join("volumes/share");
    fs::File::create(share.join("filler"))
        .unwrap()
        .set_len(8_000_000_000 - 200)
        .unwrap();
    let mut chunked = socket_upload(&a, "share", "/big.bin", "Transfer-Encoding: chunked").await;
    chunked.write_all(b"100\r\n").await.unwrap();
    chunked.write_all(&[0; 256]).await.unwrap();
    chunked.write_all(b"\r\n0\r\n\r\n").await.unwrap();
    too_large(chunked).await;
    assert_eq!(
        check(a.get("/api/storage").await, "getStorage", 200).await[1]["free"],
        136
    );
    assert!(!share.join("big.bin").exists());
    fs::write(sd.join("partial.bin"), "old").unwrap();
    let mut sock = socket_upload(&a, "sd", "/images/partial.bin", "Content-Length: 200000").await;
    sock.write_all(&vec![0; 100000]).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    drop(sock);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(fs::read(sd.join("partial.bin")).unwrap(), b"old");
    assert!(!fs::read_dir(sd).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".kvmd-upload-")));
}

#[tokio::test]
async fn mount_eject_and_protect_images() {
    let a = Api::new().await;
    assert_eq!(
        check(a.get("/api/media").await, "getMedia", 200).await,
        json!({"mounted":null,"readonly":true})
    );
    for (path, mode, code) in [
        ("/images/notes.txt", "disk", 422),
        ("/backup/pc-2026-10.img", "cdrom", 422),
        ("/images/none.iso", "cdrom", 404),
    ] {
        let out = check(
            a.post(
                "/api/media/mount",
                json!({"root":"sd","path":path,"mode":mode}),
            )
            .await,
            "mountMedia",
            code,
        )
        .await;
        if code == 422 {
            assert_eq!(out["error"], "bad_image");
        }
    }
    let m = json!({"root":"sd","path":"/images/ubuntu-24.04.iso","mode":"cdrom"});
    check(
        a.post("/api/media/mount", m.clone()).await,
        "mountMedia",
        200,
    )
    .await;
    assert_eq!(
        check(a.get("/api/media").await, "getMedia", 200).await,
        json!({"mounted":m,"readonly":true})
    );
    assert_eq!(
        a.s.app.storage.status(),
        json!({"mounted":"ubuntu-24.04.iso","mode":"cdrom"})
    );
    let ls = check(a.list("sd", "/images").await, "listFiles", 200).await;
    assert_eq!(
        ls["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == "ubuntu-24.04.iso")
            .unwrap()["mounted"],
        true
    );
    assert_eq!(
        check(
            a.post("/api/media/mount", m.clone()).await,
            "mountMedia",
            409
        )
        .await["error"],
        "already_mounted"
    );
    for (op, id, b, code, why) in [
        (
            "delete",
            "deletePath",
            json!({"root":"sd","path":"/images"}),
            409,
            "in_use",
        ),
        (
            "rename",
            "renamePath",
            json!({"root":"sd","path":"/images","to":"/imgs"}),
            409,
            "in_use",
        ),
        (
            "rename",
            "renamePath",
            json!({"root":"sd","path":"/backup","to":"/backup/inner"}),
            400,
            "bad_path",
        ),
    ] {
        assert_eq!(
            check(a.post(&format!("/api/files/{op}"), b).await, id, code).await["error"],
            why
        );
    }
    for (path, why) in [
        ("/images", "is_dir"),
        ("/images/ubuntu-24.04.iso", "in_use"),
    ] {
        assert_eq!(
            check(a.upload("sd", path, b"x", None).await, "uploadFile", 409).await["error"],
            why
        );
    }
    check(
        a.post("/api/media/eject", Value::Null).await,
        "ejectMedia",
        204,
    )
    .await;
    assert!(check(a.get("/api/media").await, "getMedia", 200).await["mounted"].is_null());
    let disk = check(
        a.post(
            "/api/media/mount",
            json!({"root":"sd","path":"/backup/pc-2026-10.img","mode":"disk","readonly":false}),
        )
        .await,
        "mountMedia",
        200,
    )
    .await;
    assert_eq!(disk["readonly"], false);
}

#[tokio::test]
async fn download_control_and_atomic_completion() {
    let a = Api::new().await;
    let args = |url: &str, path: &str| json!({"url":url,"path":path});
    assert_eq!(
        check(
            a.post(
                "/api/media/download",
                args("ftp://x/y.iso", "/images/y.iso")
            )
            .await,
            "startDownload",
            400
        )
        .await["error"],
        "bad_url"
    );
    let b = args(
        "https://releases.example.org/debian.iso",
        "/images/debian.iso",
    );
    let job = check(
        a.post("/api/media/download", b.clone()).await,
        "startDownload",
        202,
    )
    .await["job"]
        .clone();
    assert_eq!(job["path"], "/images/debian.iso");
    assert_eq!(job["state"], "running");
    let id = job["id"].as_str().unwrap();
    for (action, code, want) in [
        ("pause", 200, "paused"),
        ("pause", 409, "bad_state"),
        ("resume", 200, "running"),
        ("cancel", 200, "canceled"),
        ("resume", 409, "bad_state"),
    ] {
        let res = check(
            a.post(&format!("/api/media/jobs/{id}/{action}"), Value::Null)
                .await,
            "controlJob",
            code,
        )
        .await;
        assert_eq!(res[if code == 200 { "state" } else { "error" }], want);
    }
    let jobs = check(a.get("/api/media/jobs").await, "listJobs", 200).await;
    assert_eq!(jobs["jobs"][0]["id"], id);
    assert_eq!(jobs["jobs"][0]["state"], "canceled");
    assert!(
        !names(&check(a.list("sd", "/images").await, "listFiles", 200).await)
            .contains(&"debian.iso")
    );
    check(
        a.post("/api/media/download", b.clone()).await,
        "startDownload",
        202,
    )
    .await;
    let mut wrong = args("http://example.org/bad.img", "/images/bad.img");
    wrong["sha256"] = json!("0".repeat(64));
    check(
        a.post("/api/media/download", wrong).await,
        "startDownload",
        202,
    )
    .await;
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let jobs = check(a.get("/api/media/jobs").await, "listJobs", 200).await;
            if jobs["jobs"][1]["state"] == "done" && jobs["jobs"][2]["state"] == "failed" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    let img = a.s.dir.path().join("volumes/sd/images");
    assert!(!img.join("bad.img").exists());
    assert_eq!(
        fs::metadata(img.join("debian.iso")).unwrap().len(),
        64 << 20
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_blocked_lun_operation_does_not_stall_the_file_service() {
    let a = Api::new().await;
    let s = a.s.app.storage.clone();
    let (started, waiting) = tokio::sync::oneshot::channel();
    let (unlock, wait) = std::sync::mpsc::channel();
    let gadget = tokio::task::spawn_blocking(move || {
        s.with_gadget_args(true, |_| {
            started.send(()).unwrap();
            let _ = wait.recv_timeout(Duration::from_secs(2));
            Ok(())
        })
        .unwrap()
    });
    waiting.await.unwrap();
    let mount =
        a.s.client
            .post(format!("{}/api/media/mount", a.s.url))
            .header("cookie", &a.cookie)
            .json(&json!({"root":"sd","path":"/images/ubuntu-24.04.iso","mode":"cdrom"}));
    let mount = tokio::spawn(async move { mount.send().await.unwrap() });
    let start = std::time::Instant::now();
    tokio::time::sleep(Duration::from_millis(25)).await;
    assert_eq!(a.list("sd", "/images").await.status(), 200);
    assert!(start.elapsed() < Duration::from_millis(200));
    assert!(!mount.is_finished());
    unlock.send(()).unwrap();
    gadget.await.unwrap();
    assert_eq!(mount.await.unwrap().status(), 200);
}
