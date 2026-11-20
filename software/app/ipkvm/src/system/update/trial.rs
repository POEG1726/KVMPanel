//! The immutable kvmd in the rootfs verifies an updated process. The pending
//! record survives a failed exec or power loss between the two link changes.
use crate::atomic_write;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    future::Future,
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    os::unix::fs::{symlink, MetadataExt},
    path::{Component, Path, PathBuf},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt},
    net::{TcpStream, UnixStream},
    time::{sleep, timeout},
};
use tokio_util::codec::{FramedRead, LinesCodec};
use uuid::Uuid;

const PENDING: &str = "pending.json";
const RESULT: &str = "last-update.json";

#[derive(Serialize, Deserialize)]
struct Trial {
    candidate: PathBuf,
    previous: Option<PathBuf>,
    job: String,
}
#[derive(Serialize, Deserialize)]
struct Outcome {
    job: String,
    healthy: bool,
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> io::Result<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
fn checked_name(name: &Path) -> io::Result<()> {
    if !matches!(name.components().next(), Some(Component::Normal(_)))
        || name.file_name() != Some(name.as_os_str())
        || !name.to_str().is_some_and(|n| n.starts_with("release-"))
    {
        return Err(io::Error::other("invalid release directory name"));
    }
    Ok(())
}
fn release_link(apps: &Path, label: &str) -> io::Result<Option<PathBuf>> {
    match fs::read_link(apps.join(label)) {
        Ok(name) => {
            checked_name(&name)?;
            Ok(Some(name))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
fn remove_file(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}
fn set_link(apps: &Path, label: &str, release: Option<&Path>) -> io::Result<()> {
    let link = apps.join(label);
    if let Some(name) = release {
        checked_name(name)?;
        let next = apps.join(format!(".link-{}", Uuid::new_v4()));
        symlink(name, &next)?;
        if let Err(err) = fs::rename(&next, &link) {
            let _ = fs::remove_file(next);
            return Err(err);
        }
    } else {
        remove_file(&link)?;
    }
    fs::File::open(apps)?.sync_all()
}
pub(super) fn activate(apps: &Path, stage: tempfile::TempDir, job: &str) -> io::Result<()> {
    // Caller holds the apps directory lock from staging through publication.
    if apps.join(PENDING).try_exists()? {
        return Err(io::Error::other("update already pending"));
    }
    let previous = release_link(apps, "current")?;
    let name = PathBuf::from(format!("release-{}", Uuid::new_v4()));
    // Publish the journal before replacing current; either side of the rename
    // can therefore be recovered during the next init sequence.
    fs::rename(stage.path(), apps.join(&name))?;
    let record = Trial {
        previous,
        candidate: name,
        job: job.to_owned(),
    };
    let journal = apps.join(PENDING);
    if let Err(e) = atomic_write(&journal, &serde_json::to_vec(&record)?)
        .and_then(|_| set_link(apps, "current", Some(&record.candidate)))
    {
        // If the rename succeeded but fsync failed, keep the journal and
        // bootable candidate for init to verify on the next start.
        if release_link(apps, "current")?.as_ref() != Some(&record.candidate) {
            remove_file(&journal)?;
            fs::remove_dir_all(apps.join(record.candidate))?;
        }
        return Err(e);
    }
    Ok(())
}
pub(super) fn is_pending(data: &Path) -> bool {
    data.join("apps").join(PENDING).exists()
}
pub(super) fn pending_job(data: &Path) -> io::Result<Option<String>> {
    Ok(read::<Trial>(&data.join("apps").join(PENDING))?.map(|trial| trial.job))
}
impl Outcome {
    fn job_state(&self) -> Value {
        json!({"id":self.job,"state":if self.healthy{"done"}else{"failed"},"progress":if self.healthy{100}else{0}})
    }
}
pub(super) fn settled_job(data: &Path, id: &str) -> io::Result<Option<Value>> {
    Ok(read::<Outcome>(&data.join("apps").join(RESULT))?
        .filter(|r| r.job == id)
        .map(|r| r.job_state()))
}
pub(super) fn settle_jobs(data: &Path, jobs: &mut BTreeMap<String, Value>) -> io::Result<bool> {
    let Some(result) = read::<Outcome>(&data.join("apps").join(RESULT))? else {
        return Ok(false);
    };
    let Some(job) = jobs.get_mut(&result.job) else {
        return Ok(false);
    };
    let completed = result.job_state();
    let changed = *job != completed;
    *job = completed;
    Ok(changed)
}

fn prune(apps: &Path) -> io::Result<()> {
    // Only remove release directories created by kvmd. Keep the old previous
    // until the new current has actually proved healthy.
    let keep_current = release_link(apps, "current")?;
    let keep_previous = release_link(apps, "previous")?;
    for entry in fs::read_dir(apps)? {
        let entry = entry?;
        let name = PathBuf::from(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir()
            && (checked_name(&name).is_ok()
                || name
                    .as_os_str()
                    .as_encoded_bytes()
                    .starts_with(b".incoming-"))
            && Some(&name) != keep_current.as_ref()
            && Some(&name) != keep_previous.as_ref()
        {
            fs::remove_dir_all(entry.path())?;
        } else if kind.is_symlink() && name.as_os_str().as_encoded_bytes().starts_with(b".link-") {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

pub(super) fn cleanup_on_boot(data: &Path) -> io::Result<()> {
    let apps = data.join("apps");
    let lock = match fs::File::open(&apps) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    lock.lock()?;
    if apps.join(PENDING).try_exists()? {
        return Ok(());
    }
    if let Some(current) = release_link(&apps, "current")? {
        // Old versions had only a current link. For that layout retain the
        // newest other complete bundle as the previous release.
        if release_link(&apps, "previous")?.is_none() {
            let mut candidates = Vec::new();
            for entry in fs::read_dir(&apps)? {
                let entry = entry?;
                let name = PathBuf::from(entry.file_name());
                if checked_name(&name).is_err() || name == current || !entry.file_type()?.is_dir() {
                    continue;
                }
                let path = entry.path();
                if ["kvmd", "panel", "web/index.html"]
                    .iter()
                    .all(|file| path.join(file).is_file())
                {
                    candidates.push((entry.metadata()?.modified()?, name));
                }
            }
            if let Some((_, name)) = candidates.into_iter().max() {
                set_link(&apps, "previous", Some(&name))?;
            }
        }
    }
    prune(&apps)?;
    lock.sync_all()
}

fn finish(apps: &Path, trial: &Trial, passed: bool) -> io::Result<()> {
    let lock = fs::File::open(apps)?;
    lock.lock()?;
    let pending = read::<Trial>(&apps.join(PENDING))?;
    if !pending.is_some_and(|p| p.job == trial.job && p.candidate == trial.candidate) {
        return Err(io::Error::other("superseded update trial"));
    }
    let current = release_link(apps, "current")?;
    if current.as_ref() == Some(&trial.candidate) {
        if passed {
            set_link(apps, "previous", trial.previous.as_deref())?;
        } else {
            set_link(apps, "current", trial.previous.as_deref())?;
        }
    } else if passed || current != trial.previous {
        return Err(io::Error::other("current release changed during trial"));
    }
    prune(apps)?;
    atomic_write(
        &apps.join(RESULT),
        &serde_json::to_vec(&Outcome {
            job: trial.job.clone(),
            healthy: passed,
        })?,
    )?;
    remove_file(&apps.join(PENDING))?;
    lock.sync_all()
}

async fn panel_enabled(stream: impl AsyncRead + AsyncWrite + Unpin) -> io::Result<bool> {
    let (reader, mut writer) = tokio::io::split(stream);
    writer
        .write_all(b"{\"t\":\"get_config\",\"id\":1}\n")
        .await?;
    let mut lines = FramedRead::new(reader, LinesCodec::new_with_max_length(1024 * 1024));
    while let Some(line) = lines.next().await {
        let reply: Value = serde_json::from_str(&line.map_err(io::Error::other)?)?;
        if reply["t"] == "result" && reply["id"] == 1 && reply["ok"] == true {
            return reply
                .pointer("/data/kvm/enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| io::Error::other("invalid panel configuration"));
        }
    }
    Err(io::Error::other("panel disconnected"))
}
async fn healthy(data: &Path, panel: &str, candidate: &Path) -> io::Result<()> {
    let expected = fs::metadata(candidate)?;
    let check_pid = |pid: u32| -> io::Result<()> {
        let exe = fs::metadata(format!("/proc/{pid}/exe"))?;
        if exe.dev() != expected.dev() || exe.ino() != expected.ino() {
            return Err(io::Error::other("panel is not served by candidate"));
        }
        Ok(())
    };
    let enabled = if panel.starts_with('/') {
        let stream = UnixStream::connect(panel).await?;
        check_pid(
            stream
                .peer_cred()?
                .pid()
                .ok_or_else(|| io::Error::other("missing panel peer PID"))? as u32,
        )?;
        panel_enabled(stream).await?
    } else {
        let addr: SocketAddr = panel.parse().map_err(io::Error::other)?;
        if !addr.ip().is_loopback() {
            return Err(io::Error::other("untrusted panel listener"));
        }
        let pid = fs::read_to_string("/run/kvmd/kvmd.pid")?;
        check_pid(pid.trim().parse().map_err(io::Error::other)?)?;
        panel_enabled(TcpStream::connect(addr).await?).await?
    };
    if enabled {
        // The board always binds its configured HTTP endpoint, including when
        // HTTPS is enabled on a second port. A disabled KVM only serves panel.
        let cfg = read::<super::super::Settings>(&data.join("system.json"))?
            .unwrap_or_default()
            .network;
        let bind: IpAddr = cfg["bind"]
            .as_str()
            .ok_or_else(|| io::Error::other("missing bind address"))?
            .parse()
            .map_err(io::Error::other)?;
        let ip = match bind {
            IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
            ip => ip,
        };
        let port = cfg["http_port"]
            .as_u64()
            .and_then(|p| u16::try_from(p).ok())
            .ok_or_else(|| io::Error::other("missing HTTP port"))?;
        let response = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(io::Error::other)?
            .get(format!("http://{}/api/health", SocketAddr::new(ip, port)))
            .send()
            .await
            .map_err(io::Error::other)?;
        if !response.status().is_success()
            || response.json::<Value>().await.map_err(io::Error::other)?["ok"] != true
        {
            return Err(io::Error::other("candidate health check failed"));
        }
    }
    Ok(())
}

async fn verify<F, Fut>(apps: &Path, check: F) -> io::Result<()>
where
    F: Fn() -> Fut,
    Fut: Future<Output = io::Result<()>>,
{
    let Some(trial) = read::<Trial>(&apps.join(PENDING))? else {
        return Ok(());
    };
    checked_name(&trial.candidate)?;
    if let Some(previous) = &trial.previous {
        checked_name(previous)?;
    }
    let passed = release_link(apps, "current")?.as_ref() == Some(&trial.candidate)
        && timeout(Duration::from_secs(60), async {
            loop {
                if matches!(timeout(Duration::from_secs(2), check()).await, Ok(Ok(()))) {
                    break;
                }
                sleep(Duration::from_millis(500)).await;
            }
        })
        .await
        .is_ok();
    finish(apps, &trial, passed)?;
    if passed {
        Ok(())
    } else {
        Err(io::Error::other(
            "candidate failed health check; restored previous application",
        ))
    }
}

pub async fn verify_update(data: &Path, panel: &str) -> io::Result<()> {
    let apps = data.join("apps");
    let candidate = apps.join("current/kvmd");
    verify(&apps, || healthy(data, panel, &candidate)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage(apps: &Path, id: &str) -> PathBuf {
        let candidate = tempfile::Builder::new()
            .prefix(".incoming-")
            .tempdir_in(apps)
            .unwrap();
        fs::write(candidate.path().join("kvmd"), id).unwrap();
        activate(apps, candidate, id).unwrap();
        release_link(apps, "current").unwrap().unwrap()
    }
    fn releases(apps: &Path) -> usize {
        fs::read_dir(apps)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_type().unwrap().is_dir() && checked_name(Path::new(&e.file_name())).is_ok()
            })
            .count()
    }

    #[tokio::test(start_paused = true)]
    async fn boot_verifier_rolls_back_a_hung_candidate_after_sixty_seconds() {
        let dir = tempfile::tempdir().unwrap();
        let apps = dir.path();
        let first = stage(apps, "first");
        verify(apps, || async { Ok(()) }).await.unwrap();
        let next = stage(apps, "next");
        let start = tokio::time::Instant::now();
        // A hung service accepts connects but never answers the health request.
        assert!(verify(apps, std::future::pending::<io::Result<()>>)
            .await
            .is_err());
        assert_eq!(start.elapsed(), Duration::from_secs(60));
        assert_eq!(
            release_link(apps, "current").unwrap().as_ref(),
            Some(&first)
        );
        assert!(!apps.join(next).exists());
        let result = read::<Outcome>(&apps.join(RESULT)).unwrap().unwrap();
        assert_eq!(result.job, "next");
        assert!(!result.healthy);
    }

    #[tokio::test(start_paused = true)]
    async fn first_failed_update_restores_the_factory_and_survives_interrupted_activation() {
        let dir = tempfile::tempdir().unwrap();
        let apps = dir.path();
        stage(apps, "first");
        assert!(verify(apps, || async {
            Err(io::ErrorKind::ConnectionRefused.into())
        })
        .await
        .is_err());
        assert!(release_link(apps, "current").unwrap().is_none());
        assert_eq!(releases(apps), 0);

        let good = stage(apps, "good");
        verify(apps, || async { Ok(()) }).await.unwrap();
        let bad = stage(apps, "interrupted");
        // Simulate a power loss after the journal, before switching current.
        set_link(apps, "current", Some(&good)).unwrap();
        assert!(verify(apps, || async {
            panic!("unactivated candidate cannot pass")
        })
        .await
        .is_err());
        assert_eq!(release_link(apps, "current").unwrap().as_ref(), Some(&good));
        assert!(!apps.join(bad).exists());
    }

    #[tokio::test]
    async fn accepted_updates_retain_only_current_and_previous_complete_bundles() {
        let dir = tempfile::tempdir().unwrap();
        let apps = dir.path();
        let mut old = None;
        for n in 0..4 {
            let current = stage(apps, &n.to_string());
            verify(apps, || async { Ok(()) }).await.unwrap();
            assert_eq!(
                release_link(apps, "current").unwrap(),
                Some(current.clone())
            );
            assert_eq!(release_link(apps, "previous").unwrap(), old);
            assert_eq!(releases(apps), if n == 0 { 1 } else { 2 });
            assert!(!apps.join(PENDING).exists());
            old = Some(current);
        }
    }
}
