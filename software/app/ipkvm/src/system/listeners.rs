//! Manage the board's web listeners independently of the panel socket.
use crate::{ApiError, App};
use axum::Router;
use axum_server::{tls_rustls::RustlsConfig, Handle};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read},
    net::{IpAddr, SocketAddr, TcpListener},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{net::TcpSocket, task::JoinHandle};

type Addresses = BTreeMap<SocketAddr, Option<RustlsConfig>>;

fn read_pem(path: &Path) -> io::Result<Vec<u8>> {
    // Opening a configured device or FIFO must never block the HTTP handler.
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("expected regular PEM file"));
    }
    let mut buf = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut buf)?;
    if buf.len() > 1024 * 1024 {
        return Err(io::Error::other("PEM file exceeds 1 MiB"));
    }
    Ok(buf)
}

fn default_cert(dir: &Path, host: &str) -> io::Result<(PathBuf, PathBuf)> {
    let tls = dir.join("tls");
    if !tls.exists() {
        let stage = tempfile::tempdir_in(dir)?;
        let identity = rcgen::generate_simple_self_signed(vec![
            host.to_owned(),
            format!("{host}.local"),
            "localhost".into(),
        ])
        .map_err(io::Error::other)?;
        crate::atomic_write(
            &stage.path().join("cert.pem"),
            identity.cert.pem().as_bytes(),
        )?;
        crate::atomic_write(
            &stage.path().join("key.pem"),
            identity.signing_key.serialize_pem().as_bytes(),
        )?;
        match fs::rename(stage.path(), &tls) {
            Ok(()) => File::open(dir)?.sync_all()?,
            // Another request may have provisioned the identical default location.
            Err(_) if tls.is_dir() => {}
            Err(e) => return Err(e),
        }
    }
    Ok((tls.join("cert.pem"), tls.join("key.pem")))
}
async fn tls_config(app: &App, https: &Value) -> Result<RustlsConfig, ApiError> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = app.data_dir.clone();
    let host = app.config()["device"]["hostname"]
        .as_str()
        .unwrap_or("kvmpanel")
        .to_string();
    let pair = match (https["cert_path"].as_str(), https["key_path"].as_str()) {
        (None, None) => None,
        (Some(c), Some(k)) if !c.is_empty() && !k.is_empty() => {
            Some((PathBuf::from(c), PathBuf::from(k)))
        }
        (Some(_), None) => return Err(super::bad_body("/https/key_path")),
        _ => return Err(super::bad_body("/https/cert_path")),
    };
    let (cert, key) = tokio::task::spawn_blocking(move || -> Result<_, ApiError> {
        let (cert, key) = match pair {
            Some(p) => p,
            None => default_cert(&dir, &host)?,
        };
        Ok((
            read_pem(&cert).map_err(|_| super::bad_body("/https/cert_path"))?,
            read_pem(&key).map_err(|_| super::bad_body("/https/key_path"))?,
        ))
    })
    .await
    .map_err(io::Error::other)??;
    RustlsConfig::from_pem(cert, key)
        .await
        .map_err(|_| super::bad_body("/https"))
}

async fn addresses(app: &App, cfg: &Value) -> Result<Addresses, ApiError> {
    let ip: IpAddr = cfg["bind"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| super::bad_body("/bind"))?;
    let port = |v: &Value| {
        v.as_u64()
            .and_then(|n| u16::try_from(n).ok())
            .filter(|n| *n != 0)
    };
    let http = port(&cfg["http_port"]).ok_or_else(|| super::bad_body("/http_port"))?;
    let mut specs = BTreeMap::from([(SocketAddr::new(ip, http), None)]);
    let https = &cfg["https"];
    if https["enabled"] == true {
        let p = port(&https["port"])
            .filter(|p| *p != http)
            .ok_or_else(|| super::bad_body("/https/port"))?;
        specs.insert(SocketAddr::new(ip, p), Some(tls_config(app, https).await?));
    }
    Ok(specs)
}

pub(super) async fn validate_settings(app: &App, cfg: &Value) -> Result<(), ApiError> {
    addresses(app, cfg).await.map(drop)
}

struct Running {
    socket: TcpListener,
    tls: Option<RustlsConfig>,
    handle: Handle<SocketAddr>,
    task: JoinHandle<()>,
}
impl Drop for Running {
    fn drop(&mut self) {
        self.handle.graceful_shutdown(Some(Duration::from_secs(2)));
    }
}
fn bind(addr: SocketAddr) -> io::Result<TcpListener> {
    let sock = if addr.is_ipv4() {
        TcpSocket::new_v4()?
    } else {
        TcpSocket::new_v6()?
    };
    sock.set_reuseaddr(true)?;
    // A wildcard binding may be narrowed to eth0, or vice versa, on the same port.
    sock.set_reuseport(true)?;
    sock.bind(addr)?;
    sock.listen(128)?.into_std()
}
fn start(socket: TcpListener, tls: Option<RustlsConfig>, router: &Router) -> io::Result<Running> {
    let addr = socket.local_addr()?;
    let handle = Handle::new();
    let service = router
        .clone()
        .into_make_service_with_connect_info::<SocketAddr>();
    let task = if let Some(config) = &tls {
        let server = axum_server::from_tcp_rustls(socket.try_clone()?, config.clone())?
            .handle(handle.clone());
        tokio::spawn(async move {
            if let Err(e) = server.serve(service).await {
                eprintln!("https {addr}: {e}");
            }
        })
    } else {
        let server = axum_server::from_tcp(socket.try_clone()?)?.handle(handle.clone());
        tokio::spawn(async move {
            if let Err(e) = server.serve(service).await {
                eprintln!("http {addr}: {e}");
            }
        })
    };
    eprintln!(
        "kvmd {} listening on {addr}",
        if tls.is_some() { "HTTPS" } else { "HTTP" }
    );
    Ok(Running {
        socket,
        tls,
        handle,
        task,
    })
}
fn reconcile(
    active: &mut BTreeMap<SocketAddr, Running>,
    specs: Addresses,
    router: &Router,
) -> io::Result<()> {
    // Acquire every socket first. If even one new bind fails, all prior endpoints
    // retain their current protocol, certificate and established connections.
    let mut staged = BTreeMap::new();
    for (addr, tls) in &specs {
        match active.get(addr) {
            Some(r) if tls.is_some() == r.tls.is_some() && !r.task.is_finished() => {}
            Some(r) => {
                staged.insert(*addr, r.socket.try_clone()?);
            }
            None => {
                staged.insert(*addr, bind(*addr)?);
            }
        }
    }
    let mut replacements = BTreeMap::new();
    for (addr, sock) in staged {
        replacements.insert(addr, start(sock, specs[&addr].clone(), router)?);
    }
    active.retain(|addr, _| specs.contains_key(addr));
    active.extend(replacements);
    for (addr, tls) in specs {
        if let (Some(old), Some(new)) = (&active[&addr].tls, tls) {
            old.reload_from_config(new.get_inner());
        }
    }
    Ok(())
}

/// A CLI override or simulator binds one fixed HTTP address. Board settings
/// reload while the independent local panel server continues accepting clients.
pub async fn serve(app: App, router: Router, listen: Option<SocketAddr>) -> io::Result<()> {
    if let Some(addr) = listen.or_else(|| app.sim.then(|| ([127, 0, 0, 1], 8800).into())) {
        let tcp = tokio::net::TcpListener::bind(addr).await?;
        eprintln!("kvmd HTTP listening on {}", tcp.local_addr()?);
        return axum::serve(
            tcp,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await;
    }
    let mut active = BTreeMap::new();
    let mut events = app.events.subscribe();
    let mut retry = tokio::time::interval(Duration::from_secs(5));
    retry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    retry.tick().await;
    let mut dirty = true;
    let mut pending = true;
    loop {
        if dirty {
            let mut cfg = app.system.network();
            let result = match addresses(&app, &cfg).await {
                Ok(specs) => reconcile(&mut active, specs, &router),
                Err(err) => Err(io::Error::other(format!("{} {:?}", err.code, err.detail))),
            };
            pending = result.is_err();
            if let Err(e) = result {
                eprintln!("web listener settings: {e}");
                // After a cold boot there is no old listener to retain. Keep the
                // configured HTTP endpoint available if only the TLS endpoint failed.
                if active.is_empty() && cfg["https"]["enabled"] == true {
                    cfg["https"]["enabled"] = Value::Bool(false);
                    if let Ok(http) = addresses(&app, &cfg).await {
                        let _ = reconcile(&mut active, http, &router);
                    }
                }
            }
            dirty = false;
        }
        tokio::select! {
            ev=events.recv()=>match ev{
                Ok(name)=>if name=="network_changed"{dirty=true},
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>dirty=true,
                Err(_)=>return Ok(()),
            },
            _=retry.tick()=>dirty=pending || active.values().any(|r|r.task.is_finished()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use serde_json::json;
    #[tokio::test]
    async fn atomic_rebind_and_tls() {
        let dir = tempfile::tempdir().unwrap();
        let app = App::new(dir.path(), true).unwrap();
        let router = Router::new().route("/", get(|| async { "ready" }));
        let reserve = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let http = reserve.local_addr().unwrap();
        let reserve_tls = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let tls = reserve_tls.local_addr().unwrap();
        let mut cfg = json!({"bind":"127.0.0.1","http_port":http.port(),"https":{"enabled":true,"port":tls.port()}});
        let mut active = BTreeMap::new();
        drop(reserve);
        drop(reserve_tls);
        reconcile(&mut active, addresses(&app, &cfg).await.unwrap(), &router).unwrap();
        let url = format!("http://{http}/");
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        assert_eq!(
            client.get(&url).send().await.unwrap().text().await.unwrap(),
            "ready"
        );
        let cert =
            reqwest::Certificate::from_pem(&fs::read(dir.path().join("tls/cert.pem")).unwrap())
                .unwrap();
        let secure = reqwest::Client::builder()
            .https_only(true)
            .add_root_certificate(cert)
            .no_proxy()
            .build()
            .unwrap();
        assert_eq!(
            secure
                .get(format!("https://localhost:{}/", tls.port()))
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap(),
            "ready"
        );
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(dir.path().join("tls/key.pem"))
                .unwrap()
                .permissions()
                .mode()
                & 0o077,
            0
        );
        let conflict = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        cfg["http_port"] = json!(conflict.local_addr().unwrap().port());
        assert!(reconcile(&mut active, addresses(&app, &cfg).await.unwrap(), &router).is_err());
        assert_eq!(client.get(&url).send().await.unwrap().status(), 200);
        drop(conflict);
        cfg["https"]["enabled"] = Value::Bool(false);
        reconcile(&mut active, addresses(&app, &cfg).await.unwrap(), &router).unwrap();
        assert_eq!(active.len(), 1);
        let new = active.keys().next().unwrap();
        assert_eq!(
            client
                .get(format!("http://{new}/"))
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        cfg["https"] = json!({"enabled":true,"port":tls.port(),"cert_path":"/missing"});
        assert!(addresses(&app, &cfg).await.is_err());
    }
}
