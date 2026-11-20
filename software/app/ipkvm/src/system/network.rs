use super::bad_body;
use crate::{atomic_write, validate, ApiError, App, JsonBody};
use axum::{extract::State, http::HeaderMap, Json};
use serde_json::Value;
use std::{
    ffi::{CStr, CString},
    fs, io,
    net::{IpAddr, Ipv4Addr},
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::Path,
};
use tokio::process::Command;

pub(super) fn eth_ip() -> Option<Ipv4Addr> {
    unsafe {
        let mut head = std::ptr::null_mut();
        if libc::getifaddrs(&mut head) != 0 {
            return None;
        }
        let mut it = head;
        let mut ip = None;
        while !it.is_null() {
            let a = &*it;
            if !a.ifa_addr.is_null()
                && (*a.ifa_addr).sa_family as i32 == libc::AF_INET
                && CStr::from_ptr(a.ifa_name).to_bytes() == b"eth0"
            {
                ip = Some(Ipv4Addr::from(
                    (*(a.ifa_addr.cast::<libc::sockaddr_in>()))
                        .sin_addr
                        .s_addr
                        .to_ne_bytes(),
                ));
                break;
            }
            it = a.ifa_next;
        }
        libc::freeifaddrs(head);
        ip
    }
}
pub(super) fn space(path: &Path) -> io::Result<(u64, u64)> {
    let name = CString::new(path.as_os_str().as_bytes())?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(name.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let stat = unsafe { stat.assume_init() };
    let frag = stat.f_frsize as u64;
    Ok((
        (stat.f_blocks - stat.f_bfree) as u64 * frag,
        stat.f_blocks as u64 * frag,
    ))
}

pub(super) async fn get(State(app): State<App>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    app.require(&h, "network")?;
    Ok(Json(app.system.network()))
}

fn ifconfig(cfg: &Value) -> String {
    if cfg["mode"] == "dhcp" {
        return if cfg["dns"]["mode"] == "manual" {
            "iface eth0 inet dhcp\n  script /run/kvmd/udhcpc-manual\n"
        } else {
            "iface eth0 inet dhcp\n"
        }
        .into();
    }
    format!(
        "iface eth0 inet static\n  address {}\n  netmask {}\n  gateway {}\n",
        cfg["ip"].as_str().unwrap(),
        cfg["mask"].as_str().unwrap(),
        cfg["gateway"].as_str().unwrap()
    )
}

fn write_interfaces(path: &Path, cfg: &Value) -> io::Result<()> {
    if cfg["mode"] == "dhcp" && cfg["dns"]["mode"] == "manual" {
        let servers = cfg["dns"]["servers"]
            .as_array()
            .ok_or_else(|| io::Error::other("missing DNS servers"))?
            .iter()
            .map(|v| {
                v.as_str()
                    .and_then(|s| s.parse::<IpAddr>().ok())
                    .map(|ip| ip.to_string())
                    .ok_or_else(|| io::Error::other("invalid DNS address"))
            })
            .collect::<io::Result<Vec<_>>>()?;
        fs::create_dir_all("/run/kvmd")?;
        // udhcpc runs this hook at lease renewal too; the board's own script
        // still configures address, routes and per-interface resolver entries.
        let script = format!(
            "#!/bin/sh\nexport dns='{}'\nexec /usr/share/udhcpc/default.script \"$@\"\n",
            servers.join(" ")
        );
        let hook = Path::new("/run/kvmd/udhcpc-manual");
        atomic_write(hook, script.as_bytes())?;
        fs::set_permissions(hook, fs::Permissions::from_mode(0o700))?;
    }
    atomic_write(path, ifconfig(cfg).as_bytes())
}

pub(super) async fn apply(app: &App, cfg: &Value) -> io::Result<()> {
    let _lock = app.system.net_apply.lock().await;
    if &app.system.network() != cfg {
        return Ok(());
    }
    let path = app.data_dir.join("interfaces");
    // With the board's BusyBox init, ifupdown also manages the udhcpc process.
    let mut down = Command::new("ifdown");
    if path.exists() {
        down.arg("-i").arg(&path);
    }
    let _ = down.arg("eth0").status().await?;
    let files = path.clone();
    let settings = cfg.clone();
    tokio::task::spawn_blocking(move || write_interfaces(&files, &settings))
        .await
        .map_err(io::Error::other)??;
    let rc = Command::new("ifup")
        .arg("-f")
        .arg("-i")
        .arg(&path)
        .arg("eth0")
        .status()
        .await?;
    if !rc.success() {
        return Err(io::Error::other("ifup eth0 failed"));
    }
    if cfg["mode"] == "static" && cfg["dns"]["mode"] == "manual" {
        let mut dns = String::new();
        for addr in cfg["dns"]["servers"].as_array().into_iter().flatten() {
            dns.push_str(&format!("nameserver {} # eth0\n", addr.as_str().unwrap()));
        }
        tokio::task::spawn_blocking(move || {
            let resolv =
                fs::canonicalize("/etc/resolv.conf").unwrap_or_else(|_| "/etc/resolv.conf".into());
            atomic_write(&resolv, dns.as_bytes())
        })
        .await
        .map_err(io::Error::other)??;
    }
    Ok(())
}

pub(super) async fn put(
    State(app): State<App>,
    h: HeaderMap,
    JsonBody(body): JsonBody,
) -> Result<Json<Value>, ApiError> {
    app.require(&h, "network")?;
    validate("putNetwork", &body)?;
    for key in ["ip", "mask", "gateway"] {
        if body.get(key).is_some_and(|v| {
            v.as_str()
                .and_then(|s| s.parse::<Ipv4Addr>().ok())
                .is_none()
        }) {
            return Err(bad_body(&format!("/{key}")));
        }
    }
    if !app.sim {
        super::listeners::validate_settings(&app, &body).await?;
        if body["dns"]["servers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().and_then(|s| s.parse::<IpAddr>().ok()).is_none())
        {
            return Err(bad_body("/dns/servers"));
        }
    }
    // Reconfigure the interface even if the HTTP client leaves during fsync.
    tokio::spawn(store_and_apply(app, body))
        .await
        .map_err(io::Error::other)?
}
async fn store_and_apply(app: App, body: Value) -> Result<Json<Value>, ApiError> {
    let saved = body.clone();
    app.system
        .change(move |s| {
            s.network = saved;
            Ok(())
        })
        .await?;
    if !app.sim {
        let cfg = body.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            if let Err(e) = apply(&app, &cfg).await {
                eprintln!("applying eth0 settings: {e}");
            }
            app.emit("network_changed");
        });
    }
    Ok(Json(body))
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;
    #[test]
    fn static_interfaces() {
        assert_eq!(ifconfig(&json!({"mode":"static","ip":"10.7.0.2","mask":"255.255.0.0","gateway":"10.7.0.1"})), "iface eth0 inet static\n  address 10.7.0.2\n  netmask 255.255.0.0\n  gateway 10.7.0.1\n")
    }
}
