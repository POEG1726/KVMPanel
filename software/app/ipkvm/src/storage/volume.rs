use crate::ApiError;
use std::{
    ffi::{CString, OsStr, OsString},
    fs::{self, File, Metadata},
    io,
    os::unix::{
        ffi::OsStrExt,
        fs::MetadataExt,
        io::{AsRawFd, FromRawFd},
    },
    path::{Component, Path, PathBuf},
};

pub(super) const TEMP_PREFIX: &str = ".kvmd-upload-";

pub(super) fn error(err: io::Error) -> ApiError {
    match err.raw_os_error() {
        Some(libc::ENOENT | libc::ENOTDIR) => ApiError::new(404, "not_found"),
        Some(libc::EEXIST) => ApiError::new(409, "exists"),
        Some(libc::ENOSPC | libc::EDQUOT) => ApiError::new(507, "no_space"),
        Some(libc::ELOOP | libc::EXDEV) => ApiError::new(400, "bad_path"),
        _ => err.into(),
    }
}

pub(super) struct Target {
    pub root: String,
    pub path: PathBuf, // normalized path, relative to the volume
}

impl Target {
    pub fn new(root: &str, path: &str) -> Result<Self, ApiError> {
        let bad = || ApiError::new(400, "bad_path");
        if !matches!(root, "sd" | "share" | "system")
            || !path.starts_with('/')
            || path.contains('\0')
        {
            return Err(bad());
        }
        let mut normalized = PathBuf::new();
        for part in Path::new(path).components() {
            match part {
                Component::Normal(s) if !s.as_bytes().starts_with(TEMP_PREFIX.as_bytes()) => {
                    normalized.push(s)
                }
                Component::ParentDir if root == "system" => {
                    normalized.pop();
                }
                Component::CurDir | Component::RootDir => (),
                _ => return Err(bad()),
            }
        }
        Ok(Self {
            root: root.into(),
            path: normalized,
        })
    }

    pub fn displayed(&self) -> String {
        format!("/{}", self.path.display())
    }
}

pub(super) struct Volume {
    pub base: PathBuf,
    pub limit: Option<u64>,
}

impl Volume {
    // Anchor all path traversal in the selected root. The returned descriptor pins the
    // directory or file even when another process subsequently replaces a component.
    pub fn open(&self, path: &Path, flags: i32, system: bool) -> Result<File, ApiError> {
        #[repr(C)]
        struct OpenHow {
            flags: u64,
            mode: u64,
            resolve: u64,
        }
        const RESOLVE_NO_MAGICLINKS: u64 = 0x02;
        const RESOLVE_NO_SYMLINKS: u64 = 0x04;
        const RESOLVE_IN_ROOT: u64 = 0x10;
        let root = File::open(&self.base).map_err(error)?;
        let name = if path.as_os_str().is_empty() {
            OsStr::new(".")
        } else {
            path.as_os_str()
        };
        let name = CString::new(name.as_bytes()).map_err(|_| ApiError::new(400, "bad_path"))?;
        // The raw syscall bypasses libc's large-file handling on 32-bit ARM.
        let how = OpenHow {
            flags: (flags | libc::O_CLOEXEC | libc::O_NONBLOCK | libc::O_LARGEFILE) as u64,
            mode: 0,
            resolve: RESOLVE_IN_ROOT
                | if system {
                    RESOLVE_NO_MAGICLINKS
                } else {
                    RESOLVE_NO_SYMLINKS
                },
        };
        // SAFETY: root and both pointers remain valid for the complete syscall.
        let fd = unsafe {
            libc::syscall(
                libc::SYS_openat2,
                root.as_raw_fd(),
                name.as_ptr(),
                &how,
                size_of::<OpenHow>(),
            )
        };
        if fd < 0 {
            return Err(error(io::Error::last_os_error()));
        }
        let file = unsafe { File::from_raw_fd(fd as i32) };
        let md = file.metadata().map_err(error)?;
        if !system && md.is_file() && md.nlink() != 1 {
            return Err(ApiError::new(400, "bad_path"));
        }
        Ok(file)
    }

    pub fn parent(&self, t: &Target) -> Result<Leaf, ApiError> {
        let name = t
            .path
            .file_name()
            .ok_or_else(|| ApiError::new(404, "not_found"))?
            .to_owned();
        let dir = self.open(
            t.path.parent().unwrap(),
            libc::O_DIRECTORY,
            t.root == "system",
        )?;
        Ok(Leaf { dir, name })
    }

    pub fn capacity(&self) -> io::Result<(u64, u64)> {
        let fs = File::open(&self.base)?;
        let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        if unsafe { libc::fstatvfs(fs.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let stat = unsafe { stat.assume_init() };
        let available = (stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64);
        if let Some(limit) = self.limit {
            Ok((
                limit,
                limit.saturating_sub(used_bytes(&self.base)?).min(available),
            ))
        } else {
            Ok((
                (stat.f_blocks as u64).saturating_mul(stat.f_frsize as u64),
                available,
            ))
        }
    }
}

pub(super) fn fd_path(f: &File) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", f.as_raw_fd()))
}

pub(super) struct Leaf {
    pub dir: File,
    pub name: OsString,
}

impl Leaf {
    pub fn path(&self) -> PathBuf {
        fd_path(&self.dir).join(&self.name)
    }
    pub fn metadata(&self) -> Result<Option<Metadata>, ApiError> {
        match fs::symlink_metadata(self.path()) {
            Ok(md) if md.is_symlink() => Err(ApiError::new(400, "bad_path")),
            Ok(md) => Ok(Some(md)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(error(e)),
        }
    }
}

fn used_bytes(path: &Path) -> io::Result<u64> {
    let mut total: u64 = 0;
    for item in fs::read_dir(path)? {
        let path = item?.path();
        let md = match fs::symlink_metadata(&path) {
            Ok(md) => md,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        if md.is_dir() {
            total = total.saturating_add(used_bytes(&path)?);
        } else if md.is_file() {
            total = total.saturating_add(md.len());
        }
    }
    Ok(total)
}

pub(super) fn seed(data: &Path) -> io::Result<PathBuf> {
    let volumes = data.join("volumes");
    if volumes.is_dir() {
        return Ok(volumes);
    }
    fs::create_dir_all(data)?;
    let staging = tempfile::tempdir_in(data)?;
    let dir = staging.path();
    for folder in [
        "sd/images/drivers",
        "sd/backup",
        "share",
        "system/etc",
        "system/data/kvmd",
        "system/root",
    ] {
        fs::create_dir_all(dir.join(folder))?;
    }
    for (name, size) in [
        ("sd/images/drivers/chipset.zip", 2_000_000),
        ("sd/images/ubuntu-24.04.iso", 5_800_000_000),
        ("sd/images/win11-24h2.iso", 5_400_000_000),
        ("sd/images/bios-F12a.zip", 32_000_000),
        ("sd/backup/pc-2026-10.img", 12_165_998_000),
    ] {
        File::create(dir.join(name))?.set_len(size)?;
    }
    fs::write(dir.join("sd/images/notes.txt"), [b'n'; 2000])?;
    fs::write(dir.join("share/readme.txt"), [b'r'; 64])?;
    fs::write(dir.join("system/etc/hostname"), b"kvmpanel\n")?;
    fs::rename(dir, &volumes)?;
    Ok(volumes)
}
