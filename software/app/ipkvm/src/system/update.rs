mod trial;
use super::{bad_body, missing, VERSION};
use crate::{validate, ApiError, App, JsonBody};
use axum::{
    body::Body,
    extract::{Path as Param, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post, put},
    Json, Router,
};
use futures_util::{Stream, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Seek},
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
    time::Duration,
};
use tempfile::NamedTempFile;
use tokio::io::AsyncWriteExt;
pub use trial::verify_update;
use uuid::Uuid;

pub(super) fn recover_jobs(
    data: &Path,
    jobs: &mut std::collections::BTreeMap<String, Value>,
) -> io::Result<bool> {
    let mut changed = trial::settle_jobs(data, jobs)?;
    let pending = trial::pending_job(data)?;
    for (id, job) in jobs.iter_mut() {
        if job["state"] == "running" && pending.as_ref() != Some(id) {
            job["state"] = json!("failed");
            changed = true;
        }
    }
    Ok(changed)
}

pub(super) fn routes() -> Router<App> {
    Router::new()
        .route("/api/update", get(status))
        .route("/api/update/settings", put(settings))
        .route("/api/update/check", post(check))
        .route("/api/update/apply", post(apply))
        .route("/api/update/offline", put(offline))
        .route("/api/update/jobs/{id}", get(job))
}
fn versions(app: &App) -> Value {
    let mut v = json!({"current":VERSION});
    if app.sim {
        v["latest"] = json!("0.1.1");
    } else if let Some(latest) = app.system.latest.lock().unwrap().as_ref() {
        v["latest"] = json!(latest);
    }
    v
}
async fn status(State(app): State<App>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    app.require(&h, "updates")?;
    let mut full = app.system.settings.lock().unwrap().update.clone();
    full.as_object_mut()
        .unwrap()
        .extend(versions(&app).as_object().unwrap().clone());
    Ok(Json(full))
}
async fn settings(
    State(app): State<App>,
    h: HeaderMap,
    JsonBody(body): JsonBody,
) -> Result<Json<Value>, ApiError> {
    app.require(&h, "updates")?;
    validate("putUpdateSettings", &body)?;
    if reqwest::Url::parse(body["server"].as_str().unwrap())
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .is_none()
    {
        return Err(bad_body("/server"));
    }
    app.system.change(|s| {
        s.update = body.clone();
        Ok(())
    })?;
    *app.system.latest.lock().unwrap() = None;
    Ok(Json(body))
}
#[derive(Deserialize)]
struct Release {
    version: String,
    package: String,
    sha256: String,
}
fn client() -> io::Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(io::Error::other)
}
async fn release(app: &App) -> io::Result<(Release, reqwest::Url)> {
    let settings = app.system.settings.lock().unwrap().update.clone();
    let url = format!(
        "{}/{}.json",
        settings["server"].as_str().unwrap().trim_end_matches('/'),
        settings["channel"].as_str().unwrap()
    );
    let r = client()?
        .get(&url)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(io::Error::other)?
        .error_for_status()
        .map_err(io::Error::other)?;
    let base = r.url().clone();
    let mut bytes = Vec::new();
    let mut stream = r.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(io::Error::other)?;
        if bytes.len() + chunk.len() > 64 * 1024 {
            return Err(io::Error::other("manifest too large"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let m: Release = serde_json::from_slice(&bytes)?;
    let package = base.join(&m.package).map_err(io::Error::other)?;
    if !valid_digest(&m.sha256)
        || m.version.is_empty()
        || !matches!(package.scheme(), "https" | "http")
    {
        return Err(io::Error::other("invalid release metadata"));
    }
    Ok((m, package))
}
async fn check(State(app): State<App>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    app.require(&h, "updates")?;
    if !app.sim {
        let latest = release(&app).await.ok().map(|(v, _)| v.version);
        *app.system.latest.lock().unwrap() = latest;
    }
    Ok(Json(versions(&app)))
}

fn valid_digest(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit())
}
const MAX_PACKAGE: u64 = 256 * 1024 * 1024;
pub(super) async fn save_stream<S, E>(
    dir: &Path,
    mut stream: S,
    expected: &str,
) -> Result<NamedTempFile, ApiError>
where
    S: Stream<Item = Result<axum::body::Bytes, E>> + Unpin,
    E: std::fmt::Display,
{
    let tmp = NamedTempFile::new_in(dir)?;
    let mut f = tokio::fs::File::from_std(tmp.reopen()?);
    let mut hash = Sha256::new();
    let mut size = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| ApiError::new(400, "invalid_body"))?;
        size += chunk.len() as u64;
        if size > MAX_PACKAGE {
            return Err(ApiError::new(413, "too_large"));
        }
        f.write_all(&chunk).await?;
        hash.update(&chunk);
    }
    f.flush().await?;
    if !hex::encode(hash.finalize()).eq_ignore_ascii_case(expected) {
        return Err(ApiError::new(422, "checksum_mismatch"));
    }
    Ok(tmp)
}

async fn offline(
    State(app): State<App>,
    h: HeaderMap,
    body: Body,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    app.require(&h, "updates")?;
    let sha = h
        .get("x-sha256")
        .and_then(|v| v.to_str().ok())
        .filter(|s| valid_digest(s))
        .ok_or_else(|| bad_body("/x-sha256"))?;
    let file = save_stream(&app.data_dir, body.into_data_stream(), sha).await?;
    start(app, Some(file))
}
async fn apply(
    State(app): State<App>,
    h: HeaderMap,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    app.require(&h, "updates")?;
    start(app, None)
}
async fn job(
    State(app): State<App>,
    h: HeaderMap,
    Param(id): Param<String>,
) -> Result<Json<Value>, ApiError> {
    app.require(&h, "updates")?;
    let mut state = app.system.settings.lock().unwrap();
    trial::settle_jobs(&app.data_dir, &mut state.jobs)?;
    Ok(Json(
        json!({"job":state.jobs.get(&id).ok_or_else(missing)?}),
    ))
}

fn start(app: App, file: Option<NamedTempFile>) -> Result<(StatusCode, Json<Value>), ApiError> {
    let id = Uuid::new_v4().to_string();
    let job = json!({"id":id,"state":"running","progress":0});
    app.system.change(|s| {
        trial::settle_jobs(&app.data_dir, &mut s.jobs)?;
        if trial::is_pending(&app.data_dir) || s.jobs.values().any(|j| j["state"] == "running") {
            return Err(ApiError::new(409, "busy"));
        }
        s.jobs.insert(id.clone(), job.clone());
        Ok(())
    })?;
    tokio::spawn(async move {
        let result = if app.sim {
            Ok(())
        } else {
            install(&app, file, &id).await
        };
        if let Err(e) = &result {
            eprintln!("app update {id}: {e}");
        }
        let succeeded = result.is_ok();
        if succeeded && !app.sim {
            // The immutable init helper completes the job after checking the running release.
            let _ = super::device_action(&app, "restart_kvm");
            return;
        }
        let _ = app.system.change(|s| {
            let state = s.jobs.get_mut(&id).ok_or_else(missing)?;
            state["state"] = json!(if succeeded { "done" } else { "failed" });
            if succeeded {
                state["progress"] = json!(100);
            }
            Ok(())
        });
    });
    Ok((StatusCode::ACCEPTED, Json(json!({"job":job}))))
}
async fn install(app: &App, file: Option<NamedTempFile>, id: &str) -> io::Result<()> {
    let file = match file {
        Some(f) => f,
        None => {
            let (m, url) = release(app).await?;
            let r = client()?
                .get(url)
                .send()
                .await
                .map_err(io::Error::other)?
                .error_for_status()
                .map_err(io::Error::other)?;
            save_stream(&app.data_dir, r.bytes_stream(), &m.sha256)
                .await
                .map_err(|e| io::Error::other(e.code))?
        }
    };
    let apps = app.data_dir.join("apps");
    let job = id.to_owned();
    tokio::task::spawn_blocking(move || activate(&apps, file, &job))
        .await
        .map_err(io::Error::other)?
}

fn activate(apps: &Path, mut file: NamedTempFile, id: &str) -> io::Result<()> {
    fs::create_dir_all(apps)?;
    let stage = tempfile::Builder::new()
        .prefix(".incoming-")
        .tempdir_in(apps)?;
    file.rewind()?;
    let mut archive = tar::Archive::new(file.as_file_mut());
    let mut total = 0u64;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let mut path = PathBuf::new();
        for c in entry.path()?.components() {
            match c {
                Component::CurDir => {}
                Component::Normal(c) => path.push(c),
                _ => return Err(io::Error::other("invalid bundle path")),
            }
        }
        let kind = entry.header().entry_type();
        if path.as_os_str().is_empty() && kind.is_dir() {
            continue;
        }
        let binary = path == Path::new("kvmd") || path == Path::new("panel");
        if !binary && !path.starts_with("web") {
            return Err(io::Error::other("unexpected app bundle entry"));
        }
        let dest = stage.path().join(&path);
        if kind.is_dir() && !binary {
            fs::create_dir_all(dest)?;
            continue;
        }
        if !kind.is_file() || path == Path::new("web") {
            return Err(io::Error::other("expected regular file"));
        }
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| io::Error::other("oversized app bundle"))?;
        if total > MAX_PACKAGE {
            return Err(io::Error::other("oversized app bundle"));
        }
        fs::create_dir_all(dest.parent().unwrap())?;
        let mut output = fs::File::create_new(&dest)?;
        io::copy(&mut entry, &mut output)?;
        output.set_permissions(fs::Permissions::from_mode(if binary {
            0o755
        } else {
            0o644
        }))?;
        output.sync_all()?;
    }
    for required in ["kvmd", "panel", "web/index.html"] {
        if !stage.path().join(required).is_file() {
            return Err(io::Error::other(format!("missing {required}")));
        }
    }
    trial::activate(apps, stage, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn bundle(names: &[&str]) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        let mut tar = tar::Builder::new(f.as_file_mut());
        for name in names {
            let mut h = tar::Header::new_gnu();
            h.set_size(3);
            h.set_mode(0o777);
            h.set_cksum();
            tar.append_data(&mut h, name, &b"abc"[..]).unwrap();
        }
        tar.finish().unwrap();
        drop(tar);
        f
    }
    #[test]
    fn activates_complete_application_only() {
        let dir = tempfile::tempdir().unwrap();
        activate(
            dir.path(),
            bundle(&["kvmd", "panel", "web/index.html"]),
            "1",
        )
        .unwrap();
        let link = fs::read_link(dir.path().join("current")).unwrap();
        assert_eq!(fs::read(dir.path().join("current/kvmd")).unwrap(), b"abc");
        assert_eq!(
            fs::metadata(dir.path().join("current/kvmd"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        assert!(activate(dir.path(), bundle(&["kvmd", "etc/passwd"]), "2").is_err());
        assert!(activate(dir.path(), bundle(&["kvmd"]), "2").is_err());
        assert_eq!(fs::read_link(dir.path().join("current")).unwrap(), link);
    }
}
