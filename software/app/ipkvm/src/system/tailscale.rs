use crate::{ApiError, App};
use axum::{
    extract::{Path as Param, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use flate2::read::GzDecoder;
use serde_json::{json, Value};
use std::{
    fs, io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::process::{Child, Command};

#[derive(Default)]
pub(super) struct ActionResult {
    generation: u64,
    error: Option<String>,
}

impl ActionResult {
    fn begin(&mut self) -> u64 {
        self.generation += 1;
        self.error = None;
        self.generation
    }

    fn fail(&mut self, generation: u64, action: &str, reason: impl std::fmt::Display) {
        if self.generation == generation {
            self.error = Some(format!("{action}: {reason}"));
        }
    }
}

pub(super) struct Service {
    daemon: Option<Child>,
    auth: Option<tokio::task::JoinHandle<()>>,
    sim_installed: bool,
    sim_running: bool,
}
impl Default for Service {
    fn default() -> Self {
        Self {
            daemon: None,
            auth: None,
            sim_installed: true,
            sim_running: false,
        }
    }
}
fn directory(app: &App) -> PathBuf {
    app.data_dir.join("tailscale")
}
fn socket(app: &App) -> PathBuf {
    directory(app).join("run.sock")
}
fn cli(app: &App) -> Command {
    let mut cmd = Command::new(directory(app).join("tailscale"));
    cmd.arg(format!("--socket={}", socket(app).display()))
        .stdin(Stdio::null())
        .kill_on_drop(true);
    cmd
}
async fn run_daemon(app: &App, s: &mut Service) -> io::Result<()> {
    if let Some(child) = &mut s.daemon {
        if child.try_wait()?.is_none() {
            return Ok(());
        }
    }
    let root = directory(app);
    let mut cmd = Command::new(root.join("tailscaled"));
    let parent = std::process::id() as i32;
    // BusyBox may terminate kvmd without running destructors. Avoid leaving a
    // competing daemon with the same socket and state file after a restart.
    unsafe {
        cmd.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) < 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::getppid() != parent {
                return Err(io::Error::from(io::ErrorKind::Interrupted));
            }
            Ok(())
        });
    }
    let child = cmd
        .arg(format!(
            "--state={}",
            app.data_dir.join("tailscaled.state").display()
        ))
        .arg(format!("--socket={}", socket(app).display()))
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    s.daemon = Some(child);
    for _ in 0..80 {
        if tokio::net::UnixStream::connect(socket(app)).await.is_ok() {
            return Ok(());
        }
        if let Some(status) = s.daemon.as_mut().unwrap().try_wait()? {
            return Err(io::Error::other(format!("tailscaled: {status}")));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(io::Error::new(io::ErrorKind::TimedOut, "tailscaled socket"))
}
pub(super) async fn restore(app: &App) {
    if directory(app).join("tailscaled").is_file() {
        if let Err(e) = run_daemon(app, &mut *app.system.tail.lock().await).await {
            eprintln!("tailscaled: {e}");
        }
    }
}

async fn fetch(app: &App) -> io::Result<()> {
    let dest = directory(app);
    if dest.join("tailscale").is_file() {
        return Ok(());
    }
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(io::Error::other)?;
    const BASE: &str = "https://pkgs.tailscale.com/stable/";
    let metadata: Value = client
        .get(format!("{BASE}?mode=json"))
        .send()
        .await
        .map_err(io::Error::other)?
        .error_for_status()
        .map_err(io::Error::other)?
        .json()
        .await
        .map_err(io::Error::other)?;
    let package = metadata["Tarballs"]["arm"]
        .as_str()
        .filter(|s| {
            s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                && s.ends_with("_arm.tgz")
        })
        .ok_or_else(|| io::Error::other("missing ARM Tailscale release"))?;
    let url = format!("{BASE}{package}");
    let checksum = client
        .get(format!("{url}.sha256"))
        .send()
        .await
        .map_err(io::Error::other)?
        .error_for_status()
        .map_err(io::Error::other)?
        .text()
        .await
        .map_err(io::Error::other)?;
    let digest = checksum
        .split_whitespace()
        .next()
        .ok_or_else(|| io::Error::other("missing Tailscale checksum"))?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(io::Error::other)?
        .error_for_status()
        .map_err(io::Error::other)?;
    let package = super::update::save_stream(&app.data_dir, response.bytes_stream(), digest)
        .await
        .map_err(|e| io::Error::other(e.code))?;
    tokio::task::spawn_blocking(move || unpack(&dest, package.as_file()))
        .await
        .map_err(io::Error::other)?
}
fn unpack(dest: &Path, package: &fs::File) -> io::Result<()> {
    let stage = tempfile::tempdir_in(dest.parent().unwrap())?;
    let mut archive = tar::Archive::new(GzDecoder::new(package));
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !matches!(name, "tailscale" | "tailscaled") || !entry.header().entry_type().is_file() {
            continue;
        }
        let target = stage.path().join(name);
        if entry.size() > 160 * 1024 * 1024 {
            return Err(io::Error::other("oversized Tailscale binary"));
        }
        let mut f = fs::File::create_new(&target)?;
        io::copy(&mut entry, &mut f)?;
        f.set_permissions(fs::Permissions::from_mode(0o755))?;
        f.sync_all()?;
    }
    if !stage.path().join("tailscale").is_file() || !stage.path().join("tailscaled").is_file() {
        return Err(io::Error::other("incomplete Tailscale archive"));
    }
    fs::rename(stage.path(), dest)?;
    Ok(())
}
pub(super) async fn get(
    State(app): State<App>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    app.require(&headers, "tailscale")?;
    let mut status = if app.sim {
        let s = app.system.tail.lock().await;
        json!({"installed":s.sim_installed,"running":s.sim_running})
    } else {
        json!({"installed":directory(&app).join("tailscale").is_file(),"running":false})
    };
    if !app.sim && status["installed"] == true {
        let command = cli(&app).args(["status", "--json"]).output();
        if let Ok(Ok(output)) = tokio::time::timeout(Duration::from_secs(3), command).await {
            if let Ok(data) = serde_json::from_slice::<Value>(&output.stdout) {
                status["running"] = json!(data["BackendState"] == "Running");
                if let Some(ip) = data["TailscaleIPs"].get(0).and_then(Value::as_str) {
                    status["ip"] = json!(ip);
                }
                if let Some(url) = data["AuthURL"].as_str().filter(|s| !s.is_empty()) {
                    status["login_url"] = json!(url);
                }
            }
        }
    }
    if let Some(error) = &app.system.tail_action.lock().unwrap().error {
        status["error"] = json!(error);
    }
    Ok(Json(status))
}
async fn wait_cli(child: Child) -> io::Result<()> {
    let output = child.wait_with_output().await?;
    if output.status.success() {
        return Ok(());
    }
    let reason = String::from_utf8_lossy(&output.stderr);
    Err(io::Error::other(format!(
        "{} {}",
        output.status,
        reason.trim().chars().take(512).collect::<String>()
    )))
}

async fn change(app: &App, action: &str, generation: u64) -> io::Result<()> {
    let mut s = app.system.tail.lock().await;
    if let Some(previous) = s.auth.take() {
        previous.abort();
        let _ = previous.await;
    }
    match action {
        "install" => {
            fetch(app).await?;
            run_daemon(app, &mut s).await?;
        }
        "uninstall" => {
            if let Some(mut c) = s.daemon.take() {
                let _ = c.kill().await;
            }
            let dir = directory(app);
            if dir.exists() {
                fs::remove_dir_all(dir)?;
            }
        }
        _ => {
            run_daemon(app, &mut s).await?;
            let child = cli(app)
                .arg(action)
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()?;
            let app = app.clone();
            let action = action.to_owned();
            s.auth = Some(tokio::spawn(async move {
                if let Err(error) = wait_cli(child).await {
                    app.system
                        .tail_action
                        .lock()
                        .unwrap()
                        .fail(generation, &action, &error);
                }
            }));
        }
    }
    Ok(())
}
pub(super) async fn action(
    State(app): State<App>,
    headers: HeaderMap,
    Param(act): Param<String>,
) -> Result<StatusCode, ApiError> {
    app.require(&headers, "tailscale")?;
    if !matches!(
        act.as_str(),
        "install" | "uninstall" | "up" | "down" | "login" | "logout"
    ) {
        return Err(ApiError::new(400, "bad_action"));
    }
    let generation = app.system.tail_action.lock().unwrap().begin();
    if app.sim {
        let mut s = app.system.tail.lock().await;
        match act.as_str() {
            "install" => s.sim_installed = true,
            "uninstall" => {
                s.sim_installed = false;
                s.sim_running = false;
            }
            "up" | "login" if !s.sim_installed => {
                app.system.tail_action.lock().unwrap().fail(
                    generation,
                    &act,
                    "Tailscale is not installed",
                );
            }
            "up" | "login" => s.sim_running = true,
            _ => s.sim_running = false,
        }
    } else {
        tokio::spawn(async move {
            if let Err(e) = change(&app, &act, generation).await {
                app.system
                    .tail_action
                    .lock()
                    .unwrap()
                    .fail(generation, &act, &e);
                eprintln!("tailscale {act}: {e}");
            }
        });
    }
    Ok(StatusCode::ACCEPTED)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cli_nonzero_exit_and_text_are_reported() {
        let child = Command::new("sh")
            .args(["-c", "echo unable-to-connect >&2; exit 3"])
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let err = wait_cli(child).await.unwrap_err().to_string();
        assert!(err.contains("unable-to-connect") && err.contains("3"));
    }
    #[test]
    fn late_previous_failure_does_not_replace_the_latest_result() {
        let mut state = ActionResult::default();
        let prev = state.begin();
        let current = state.begin();
        state.fail(prev, "login", "canceled");
        assert!(state.error.is_none());
        state.fail(current, "up", "missing package");
        assert_eq!(state.error.as_deref(), Some("up: missing package"));
    }
}
