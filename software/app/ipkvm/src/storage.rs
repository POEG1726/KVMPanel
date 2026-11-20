mod jobs;
mod lun;
mod share;
mod volume;

use crate::{validate, ApiError, App, JsonBody};
use axum::{
    body::Body,
    extract::{Path as RoutePath, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use futures_util::StreamExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    env,
    ffi::OsString,
    fs::{self, File, Metadata},
    io::{self, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
    time::UNIX_EPOCH,
};
use tokio_util::io::ReaderStream;
use volume::{error, fd_path, Leaf, Target, Volume, TEMP_PREFIX};

type Reply = Result<Response, ApiError>;

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(io::Error::other)?
}
const ROOTS: [&str; 3] = ["sd", "share", "system"];

struct Mount {
    path: PathBuf,
    mode: String,
    readonly: bool,
}

#[derive(Default)]
struct Runtime {
    mount: Option<Mount>,
    exported: bool,
    switching_share: bool,
    needs_recovery: bool,
    pending: HashSet<PathBuf>,
    jobs: Vec<jobs::Job>,
}

pub struct Storage {
    volumes: HashMap<String, Volume>,
    lun: Box<dyn lun::Lun>,
    state: Mutex<Runtime>,
    // Serialize gadget changes with operations on its LUNs.
    lun_gate: Mutex<()>,
    share_view: RwLock<()>,
    sim: bool,
    share_image: PathBuf,
}

fn setting(name: &str, default: &str) -> PathBuf {
    env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| default.into())
}

impl Storage {
    pub fn new(data: &Path, sim: bool) -> io::Result<Self> {
        let mut volumes = HashMap::new();
        let lun: Box<dyn lun::Lun> = if sim {
            let base = volume::seed(data)?.canonicalize()?;
            for (root, limit) in ROOTS
                .into_iter()
                .zip([58_000_000_000, 8_000_000_000, 500_000_000])
            {
                volumes.insert(
                    root.into(),
                    Volume {
                        base: base.join(root),
                        limit: Some(limit),
                    },
                );
            }
            Box::new(lun::SimLun)
        } else {
            let sd = setting("KVMD_SD_ROOT", "/mnt/sd");
            let share = setting("KVMD_SHARE_ROOT", "/mnt/share");
            for (root, base) in [
                ("sd", sd.clone()),
                ("share", share.clone()),
                ("system", setting("KVMD_SYSTEM_ROOT", "/")),
            ] {
                if !base.is_absolute() {
                    return Err(io::Error::other("storage roots must be absolute"));
                }
                let base = base.canonicalize().unwrap_or(base);
                volumes.insert(root.into(), Volume { base, limit: None });
            }
            Box::new(lun::BoardLun {
                function: "/sys/kernel/config/usb_gadget/kvmpanel/functions/mass_storage.media"
                    .into(),
                share_root: share,
                share_record: data.join("share-export.json"),
                share_image: env::var_os("KVMD_SHARE_IMAGE")
                    .map(PathBuf::from)
                    .unwrap_or(sd.join("share.img")),
            })
        };
        let (mount, exported, needs_recovery) = match lun.restore(&volumes["sd"].base) {
            Ok((mount, exported)) => (mount, exported, false),
            Err(e) => {
                eprintln!("kvmd: cannot recover USB media yet: {e}");
                (None, false, true)
            }
        };
        let share_image = if sim {
            volumes["sd"].base.join("share.img")
        } else {
            env::var_os("KVMD_SHARE_IMAGE")
                .map(PathBuf::from)
                .unwrap_or_else(|| volumes["sd"].base.join("share.img"))
        };
        if !share_image.is_absolute() {
            return Err(io::Error::other("shared image must be an absolute path"));
        }
        Ok(Self {
            share_image: share_image.canonicalize().unwrap_or(share_image),
            volumes,
            lun,
            state: Mutex::new(Runtime {
                mount,
                exported,
                needs_recovery,
                ..Default::default()
            }),
            sim,
            lun_gate: Mutex::new(()),
            share_view: RwLock::new(()),
        })
    }

    // Called under lun_gate. A card can be absent while the daemon starts or
    // an interrupted export is being restored. Retry on the next operation.
    fn recover_luns(&self) -> Result<(), ApiError> {
        if self.state.lock().unwrap().needs_recovery {
            let _view = self.share_view.write().unwrap();
            let (mount, exported) = self
                .lun
                .restore(&self.volumes["sd"].base)
                .map_err(ApiError::device)?;
            let mut state = self.state.lock().unwrap();
            state.mount = mount;
            state.exported = exported;
            state.needs_recovery = false;
        }
        Ok(())
    }
    fn ensure_ready(&self) -> Result<(), ApiError> {
        if self.state.lock().unwrap().needs_recovery {
            let _gate = self.lun_gate.lock().unwrap();
            self.recover_luns()?;
        }
        Ok(())
    }

    pub fn with_gadget_args(
        &self,
        enable_storage: bool,
        apply: impl FnOnce(Vec<(OsString, OsString)>) -> Result<(), ApiError>,
    ) -> Result<(), ApiError> {
        let _gate = self.lun_gate.lock().unwrap();
        self.recover_luns()?;
        let state = self.state.lock().unwrap();
        if !enable_storage && (state.exported || state.mount.is_some()) {
            return Err(ApiError::new(409, "in_use"));
        }
        let args = self.lun.args(
            state.mount.as_ref(),
            &self.volumes["sd"].base,
            state.exported,
        );
        drop(state);
        apply(args)
    }

    pub fn status(&self) -> Value {
        let state = self.state.lock().unwrap();
        match &state.mount {
            Some(m) => {
                json!({"mounted":m.path.file_name().unwrap().to_string_lossy(),"mode":m.mode})
            }
            None => json!({"mounted":null,"mode":null}),
        }
    }

    pub fn eject(&self) -> Result<(), ApiError> {
        let _gate = self.lun_gate.lock().unwrap();
        self.recover_luns()?;
        let mut state = self.state.lock().unwrap();
        if let Some(m) = &state.mount {
            self.lun
                .bind(None, &m.mode, true)
                .map_err(ApiError::device)?;
        }
        state.mount = None;
        Ok(())
    }

    fn path(&self, t: &Target) -> PathBuf {
        self.volumes[&t.root].base.join(&t.path)
    }

    // Compare physical paths as well as root names, since the admin system root
    // can expose the very same SD directory and share mount.
    fn ensure_writable(&self, state: &Runtime, path: &Path) -> Result<(), ApiError> {
        if state.needs_recovery {
            return Err(ApiError::new(503, "device_unavailable"));
        }
        let same_file = |p: &Path| -> bool {
            match (fs::metadata(path), fs::metadata(p)) {
                (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
                _ => false,
            }
        };
        if self.share_image.starts_with(path) || same_file(&self.share_image) {
            return Err(ApiError::new(
                409,
                if state.exported {
                    "share_busy"
                } else {
                    "in_use"
                },
            ));
        }
        if (state.exported || state.switching_share)
            && (path.starts_with(&self.volumes["share"].base)
                || self.volumes["share"].base.starts_with(path))
        {
            return Err(ApiError::new(409, "share_busy"));
        }
        if state.mount.as_ref().is_some_and(|m| {
            let image = self.volumes["sd"].base.join(&m.path);
            image.starts_with(path) || same_file(&image)
        }) || state.pending.iter().any(|p| p.starts_with(path))
        {
            return Err(ApiError::new(409, "in_use"));
        }
        Ok(())
    }
}

pub fn routes() -> Router<App> {
    Router::new()
        .route("/api/storage", get(capacity))
        .route("/api/files", get(list))
        .route("/api/files/download", get(download))
        .route("/api/files/upload", put(upload))
        .route("/api/files/mkdir", post(mkdir))
        .route("/api/files/rename", post(rename))
        .route("/api/files/delete", post(delete))
        .route("/api/share", get(share).post(set_share))
        .route("/api/media", get(media))
        .route("/api/media/mount", post(mount))
        .route("/api/media/eject", post(eject))
        .route("/api/media/download", post(jobs::start))
        .route("/api/media/jobs", get(jobs::list))
        .route("/api/media/jobs/{id}/{action}", post(jobs::control))
}

fn ok(body: Value) -> Response {
    Json(body).into_response()
}
fn created(body: Value) -> Response {
    (StatusCode::CREATED, Json(body)).into_response()
}
fn vstr<'a>(b: &'a Value, k: &str) -> &'a str {
    b[k].as_str().unwrap_or_default()
}
fn file_target(app: &App, headers: &HeaderMap, root: &str, path: &str) -> Result<Target, ApiError> {
    let user = app.require(headers, "files")?;
    if root == "system" && user["role"] != "admin" {
        return Err(ApiError::new(403, "forbidden"));
    }
    Target::new(root, path)
}

type Params = Query<HashMap<String, String>>;

fn query_target(app: &App, h: &HeaderMap, q: &Params) -> Result<Target, ApiError> {
    file_target(
        app,
        h,
        q.get("root").map_or("", String::as_str),
        q.get("path").map_or("", String::as_str),
    )
}

fn write_target(app: &App, h: &HeaderMap, op: &str, b: &Value) -> Result<Target, ApiError> {
    app.require(h, "files")?;
    validate(op, b)?;
    file_target(app, h, vstr(b, "root"), vstr(b, "path"))
}

async fn capacity(State(app): State<App>, h: HeaderMap) -> Reply {
    app.require(&h, "files")?;
    blocking(move || {
        let _view = app.storage.share_view.read().unwrap();
        let mut volumes = Vec::new();
        for root in ROOTS {
            let vol = &app.storage.volumes[root];
            match vol.capacity() {
                Ok((total, free)) => {
                    volumes.push(json!({"root":root,"total":total,"free":free,"mounted":true}))
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    volumes.push(json!({"root":root,"total":0,"free":0,"mounted":false}))
                }
                Err(e) => return Err(error(e)),
            }
        }
        Ok(ok(json!(volumes)))
    })
    .await
}

fn entry(name: &str, md: &Metadata, mounted: bool) -> Value {
    let time = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as u64);
    let mut e = json!({"name":name,"type":if md.is_dir(){"dir"}else{"file"},
        "size":if md.is_dir(){0}else{md.len()},"mtime":time});
    if mounted {
        e["mounted"] = json!(true);
    }
    e
}

async fn list(State(app): State<App>, h: HeaderMap, q: Params) -> Reply {
    let t = query_target(&app, &h, &q)?;
    blocking(move || {
        let s = &app.storage;
        let _view = (t.root != "sd").then(|| s.share_view.read().unwrap());
        let dir = s.volumes[&t.root].open(&t.path, libc::O_DIRECTORY, t.root == "system")?;
        let state = s.state.lock().unwrap();
        let mount = state
            .mount
            .as_ref()
            .map(|m| s.volumes["sd"].base.join(&m.path));
        drop(state);
        let mut entries = Vec::new();
        for item in fs::read_dir(fd_path(&dir)).map_err(error)? {
            let item = item.map_err(error)?;
            let name = item.file_name().to_string_lossy().into_owned();
            if name.starts_with(TEMP_PREFIX) {
                continue;
            }
            let md = fs::symlink_metadata(item.path()).map_err(error)?;
            if !md.is_dir() && !md.is_file() {
                continue;
            }
            entries.push(entry(
                &name,
                &md,
                mount.as_ref() == Some(&s.path(&t).join(name.as_str())),
            ));
        }
        entries.sort_by(|a, b| {
            vstr(a, "type")
                .cmp(vstr(b, "type"))
                .then(vstr(a, "name").cmp(vstr(b, "name")))
        });
        Ok(ok(json!({"entries":entries})))
    })
    .await
}

async fn download(State(app): State<App>, h: HeaderMap, q: Params) -> Reply {
    let t = query_target(&app, &h, &q)?;
    blocking(move || {
        let file =
            app.storage.volumes[&t.root].open(&t.path, libc::O_RDONLY, t.root == "system")?;
        let md = file.metadata().map_err(error)?;
        if !md.is_file() {
            return Err(ApiError::new(404, "not_found"));
        }
        let name = t.path.file_name().unwrap().as_encoded_bytes();
        let encoded: String = name
            .iter()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b"-._~".contains(b) {
                    char::from(*b).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect();
        let body = Body::from_stream(ReaderStream::new(tokio::fs::File::from_std(file)));
        Ok((
            [
                (header::CONTENT_TYPE, "application/octet-stream".into()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename*=UTF-8''{encoded}"),
                ),
                (header::CONTENT_LENGTH, md.len().to_string()),
            ],
            body,
        )
            .into_response())
    })
    .await
}

// An async request may be canceled between any two chunks. Keep the staging
// file and its reservation on the blocking pool, including abort cleanup.
struct AsyncTransfer(Option<Arc<Mutex<Transfer>>>);
impl AsyncTransfer {
    async fn begin(
        app: App,
        target: Target,
        size: u64,
        create_only: bool,
    ) -> Result<Self, ApiError> {
        blocking(move || {
            Ok(Self(Some(Arc::new(Mutex::new(Transfer::begin(
                &app,
                target,
                size,
                create_only,
            )?)))))
        })
        .await
    }
    fn writer(&self) -> Arc<Mutex<Transfer>> {
        self.0.as_ref().unwrap().clone()
    }
    async fn write(&self, chunk: axum::body::Bytes) -> Result<(), ApiError> {
        let writer = self.writer();
        blocking(move || writer.lock().unwrap().write(&chunk)).await
    }
    async fn finish(mut self, checksum: Option<String>) -> Result<Value, ApiError> {
        let writer = self.0.take().unwrap();
        blocking(move || writer.lock().unwrap().finish(checksum.as_deref(), false)).await
    }
}
impl Drop for AsyncTransfer {
    fn drop(&mut self) {
        if let Some(writer) = self.0.take() {
            tokio::task::spawn_blocking(move || drop(writer));
        }
    }
}

struct Transfer {
    // Drop the temporary file before the parent descriptor, which anchors its name.
    temp: tempfile::NamedTempFile,
    leaf: Leaf,
    target: Target,
    absolute: PathBuf,
    storage: std::sync::Arc<Storage>,
    hash: Sha256,
}

impl Transfer {
    fn begin(app: &App, t: Target, declared: u64, create_only: bool) -> Result<Self, ApiError> {
        let s = app.storage.clone();
        s.ensure_ready()?;
        let volume = &s.volumes[&t.root];
        let leaf = volume.parent(&t)?;
        let absolute = fs::canonicalize(fd_path(&leaf.dir))
            .map_err(error)?
            .join(&leaf.name);
        let mut state = s.state.lock().unwrap();
        if let Some(md) = leaf.metadata()? {
            if create_only {
                return Err(ApiError::new(409, "exists"));
            }
            if md.is_dir() {
                return Err(ApiError::new(409, "is_dir"));
            }
            if !md.is_file() {
                return Err(ApiError::new(400, "bad_path"));
            }
        }
        if state.switching_share {
            return Err(ApiError::new(409, "share_busy"));
        }
        s.ensure_writable(&state, &absolute)?;
        if declared > volume.capacity().map_err(error)?.1 {
            return Err(ApiError::new(507, "no_space"));
        }
        let temp = tempfile::Builder::new()
            .prefix(TEMP_PREFIX)
            .tempfile_in(fd_path(&leaf.dir))
            .map_err(error)?;
        state.pending.insert(absolute.clone());
        drop(state);
        Ok(Self {
            temp,
            leaf,
            target: t,
            absolute,
            storage: s,
            hash: Sha256::new(),
        })
    }
    fn write(&mut self, data: &[u8]) -> Result<(), ApiError> {
        let storage = self.storage.clone();
        let _state = storage.state.lock().unwrap();
        self.write_locked(data)
    }
    fn write_locked(&mut self, data: &[u8]) -> Result<(), ApiError> {
        let volume = &self.storage.volumes[&self.target.root];
        if data.len() as u64 > volume.capacity().map_err(error)?.1 {
            return Err(ApiError::new(507, "no_space"));
        }
        self.temp.write_all(data).map_err(error)?;
        self.hash.update(data);
        Ok(())
    }
    fn finish(&mut self, checksum: Option<&str>, create_only: bool) -> Result<Value, ApiError> {
        if let Some(hash) = checksum {
            if !format!("{:x}", self.hash.clone().finalize()).eq_ignore_ascii_case(hash) {
                return Err(ApiError::new(422, "checksum_mismatch"));
            }
        }
        self.temp.as_file().sync_all().map_err(error)?;
        let path = self.leaf.path();
        let dir = fd_path(&self.leaf.dir);
        // The fd stays stable across renames. Refuse publication if the requested pathname has moved.
        let dest = self.storage.volumes[&self.target.root].parent(&self.target)?;
        let d = dest.dir.metadata().map_err(error)?;
        let orig = self.leaf.dir.metadata().map_err(error)?;
        if d.ino() != orig.ino() || d.dev() != orig.dev() {
            return Err(ApiError::new(409, "in_use"));
        }
        if create_only {
            rename_new(self.temp.path(), &path)?;
        } else {
            fs::rename(self.temp.path(), &path).map_err(error)?;
        }
        File::open(dir).and_then(|f| f.sync_all()).map_err(error)?;
        Ok(entry(
            &self.leaf.name.to_string_lossy(),
            &self.temp.as_file().metadata().map_err(error)?,
            false,
        ))
    }
}

impl Drop for Transfer {
    fn drop(&mut self) {
        self.storage
            .state
            .lock()
            .unwrap()
            .pending
            .remove(&self.absolute);
    }
}

fn rename_new(from: &Path, to: &Path) -> Result<(), ApiError> {
    let cstr = |p: &Path| {
        std::ffi::CString::new(p.as_os_str().as_encoded_bytes())
            .map_err(|_| ApiError::new(400, "bad_path"))
    };
    let from = cstr(from)?;
    let to = cstr(to)?;
    let ret = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if ret == 0 {
        Ok(())
    } else {
        Err(error(io::Error::last_os_error()))
    }
}

async fn upload(State(app): State<App>, h: HeaderMap, q: Params, body: Body) -> Reply {
    let t = query_target(&app, &h, &q)?;
    let declared = h
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok()?.parse().ok())
        .unwrap_or(0);
    let checksum = h
        .get("x-sha256")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let tx = AsyncTransfer::begin(app, t, declared, false).await?;
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        tx.write(chunk.map_err(|_| ApiError::new(400, "invalid_body"))?)
            .await?;
    }
    let e = tx.finish(checksum).await?;
    Ok(created(e))
}

async fn mkdir(State(app): State<App>, h: HeaderMap, JsonBody(b): JsonBody) -> Reply {
    let t = write_target(&app, &h, "makeDir", &b)?;
    blocking(move || {
        app.storage.ensure_ready()?;
        let s = &app.storage;
        let state = s.state.lock().unwrap();
        let leaf = s.volumes[&t.root].parent(&t)?;
        s.ensure_writable(
            &state,
            &fs::canonicalize(fd_path(&leaf.dir))
                .map_err(error)?
                .join(&leaf.name),
        )?;
        fs::create_dir(leaf.path()).map_err(error)?;
        Ok(created(entry(
            &leaf.name.to_string_lossy(),
            &fs::metadata(leaf.path()).map_err(error)?,
            false,
        )))
    })
    .await
}

async fn rename(State(app): State<App>, h: HeaderMap, JsonBody(b): JsonBody) -> Reply {
    let t = write_target(&app, &h, "renamePath", &b)?;
    let to = Target::new(&t.root, vstr(&b, "to"))?;
    blocking(move || {
        app.storage.ensure_ready()?;
        let s = &app.storage;
        let state = s.state.lock().unwrap();
        if to.path.starts_with(&t.path) {
            return Err(ApiError::new(400, "bad_path"));
        }
        let vol = &s.volumes[&t.root];
        let src = vol.parent(&t)?;
        let dst = vol.parent(&to)?;
        for leaf in [&src, &dst] {
            s.ensure_writable(
                &state,
                &fs::canonicalize(fd_path(&leaf.dir))
                    .map_err(error)?
                    .join(&leaf.name),
            )?;
        }
        rename_new(&src.path(), &dst.path())?;
        Ok(StatusCode::NO_CONTENT.into_response())
    })
    .await
}

async fn delete(State(app): State<App>, h: HeaderMap, JsonBody(b): JsonBody) -> Reply {
    let t = write_target(&app, &h, "deletePath", &b)?;
    blocking(move || {
        app.storage.ensure_ready()?;
        let s = &app.storage;
        let state = s.state.lock().unwrap();
        let leaf = s.volumes[&t.root].parent(&t)?;
        s.ensure_writable(
            &state,
            &fs::canonicalize(fd_path(&leaf.dir))
                .map_err(error)?
                .join(&leaf.name),
        )?;
        let md = fs::symlink_metadata(leaf.path()).map_err(error)?;
        if md.is_dir() {
            fs::remove_dir_all(leaf.path())
        } else {
            fs::remove_file(leaf.path())
        }
        .map_err(error)?;
        Ok(StatusCode::NO_CONTENT.into_response())
    })
    .await
}

async fn share(State(app): State<App>, h: HeaderMap) -> Reply {
    app.require(&h, "files")?;
    blocking(move || {
        app.storage.ensure_ready()?;
        Ok(ok(
            json!({"exported_to_host":app.storage.state.lock().unwrap().exported}),
        ))
    })
    .await
}

async fn set_share(State(app): State<App>, h: HeaderMap, JsonBody(b): JsonBody) -> Reply {
    app.require(&h, "media")?;
    validate("setShare", &b)?;
    blocking(move || {
        let s = &app.storage;
        let _gate = s.lun_gate.lock().unwrap();
        s.recover_luns()?;
        let export = b["exported_to_host"] == true;
        {
            let mut state = s.state.lock().unwrap();
            if export == state.exported {
                return Ok(ok(json!({"exported_to_host":export})));
            }
            if !state.pending.is_empty() {
                return Err(ApiError::new(409, "share_busy"));
            }
            if export && !app.storage_enabled() {
                return Err(ApiError::new(409, "in_use"));
            }
            state.switching_share = true;
        }
        let _view = s.share_view.write().unwrap();
        let change = s.lun.share(export);
        let mut state = s.state.lock().unwrap();
        state.switching_share = false;
        if let Err(e) = change {
            state.needs_recovery = true;
            return Err(ApiError::device(e));
        }
        state.exported = export;
        Ok(ok(json!({"exported_to_host":export})))
    })
    .await
}

fn media_state(rt: &Runtime) -> Value {
    match &rt.mount {
        Some(m) => {
            json!({"mounted":{"root":"sd","path":format!("/{}",m.path.display()),"mode":m.mode},"readonly":m.readonly})
        }
        None => json!({"mounted":null,"readonly":true}),
    }
}

async fn media(State(app): State<App>, h: HeaderMap) -> Reply {
    app.require(&h, "media")?;
    blocking(move || Ok(ok(media_state(&app.storage.state.lock().unwrap())))).await
}

async fn mount(State(app): State<App>, h: HeaderMap, JsonBody(b): JsonBody) -> Reply {
    app.require(&h, "media")?;
    validate("mountMedia", &b)?;
    let t = Target::new("sd", vstr(&b, "path"))?;
    blocking(move || {
        app.storage.ensure_ready()?;
        let mode = vstr(&b, "mode");
        let s = &app.storage;
        let _gate = s.lun_gate.lock().unwrap();
        s.recover_luns()?;
        let mut state = s.state.lock().unwrap();
        if !app.storage_enabled() {
            return Err(ApiError::new(409, "in_use"));
        }
        if state.mount.is_some() {
            return Err(ApiError::new(409, "already_mounted"));
        }
        let vol = &s.volumes["sd"];
        s.ensure_writable(&state, &s.path(&t))?;
        let f = vol.open(&t.path, libc::O_RDONLY, false)?;
        if !f.metadata().map_err(error)?.is_file() {
            return Err(ApiError::new(404, "not_found"));
        }
        let ext = t
            .path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if !(ext.eq_ignore_ascii_case("iso") || mode == "disk" && ext.eq_ignore_ascii_case("img")) {
            return Err(ApiError::new(422, "bad_image"));
        }
        let readonly = mode == "cdrom" || b["readonly"].as_bool().unwrap_or(true);
        s.lun
            .bind(Some(&fd_path(&f)), mode, readonly)
            .map_err(ApiError::device)?;
        state.mount = Some(Mount {
            path: t.path,
            mode: mode.into(),
            readonly,
        });
        let out = media_state(&state);
        drop(state);
        app.emit("status_changed");
        Ok(ok(out))
    })
    .await
}

async fn eject(State(app): State<App>, h: HeaderMap) -> Reply {
    app.require(&h, "media")?;
    blocking(move || {
        app.storage.eject()?;
        app.emit("status_changed");
        Ok(StatusCode::NO_CONTENT.into_response())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_lun_surfaces_device_error_and_preserves_mounted_state() {
        let d = tempfile::tempdir().unwrap();
        let mut s = Storage::new(d.path(), true).unwrap();
        s.lun = Box::new(lun::BoardLun {
            function: d.path().join("absent"),
            share_root: d.path().join("share"),
            share_record: d.path().join("share-export.json"),
            share_image: d.path().join("share.img"),
        });
        s.state.lock().unwrap().mount = Some(Mount {
            path: "images/image.iso".into(),
            mode: "cdrom".into(),
            readonly: true,
        });
        let e = s.eject().unwrap_err();
        assert_eq!((e.status, e.code.as_str()), (503, "device_unavailable"));
        assert_eq!(s.status()["mounted"], "image.iso");
    }
}
