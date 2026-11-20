//! The PC must be the sole writer to a live USB disk. While it owns the
//! backing image, the board exposes a frozen, separately mounted filesystem.
use super::{
    lun::{mounted_device, BoardLun},
    volume::TEMP_PREFIX,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::Command,
};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
struct Export {
    image: PathBuf,
    root: PathBuf,
    snapshot: PathBuf,
    options: Vec<String>,
}

impl Export {
    fn mount_options(&self, operation: &str, write: bool) -> String {
        self.options
            .iter()
            .map(String::as_str)
            .chain([operation, if write { "rw" } else { "ro" }])
            .collect::<Vec<_>>()
            .join(",")
    }
}

fn command(cmd: &mut Command) -> io::Result<()> {
    let out = cmd.output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{}: {}",
            cmd.get_program().to_string_lossy(),
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

fn remove(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

impl BoardLun {
    fn checkpoint(&self) -> io::Result<Option<Export>> {
        let raw = match fs::read(&self.share_record) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let r: Export = serde_json::from_slice(&raw)?;
        let valid = r.image == self.share_image.canonicalize()?
            && r.root == self.share_root.canonicalize()?
            && r.snapshot.parent() == r.image.parent()
            && r.snapshot
                .file_name()
                .and_then(|s| s.to_str())
                .and_then(|n| n.strip_prefix(TEMP_PREFIX)?.strip_prefix("view-"))
                .is_some_and(|tail| Uuid::parse_str(tail).is_ok());
        if !valid {
            return Err(io::Error::other("invalid shared-disk recovery record"));
        }
        Ok(Some(r))
    }

    // An absent mount is legitimate between umount and the replacement mount.
    // A mount that is present must be the root of a loop-backed filesystem.
    fn mounted_image(&self) -> io::Result<Option<PathBuf>> {
        let at = self.share_root.canonicalize()?;
        let table = fs::read_to_string("/proc/self/mountinfo")?;
        let (dev, _) = match mounted_device(&table, &at) {
            Ok(dev) => dev,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let backing = fs::read_to_string(
            Path::new("/sys/dev/block")
                .join(dev)
                .join("loop/backing_file"),
        )?;
        Ok(Some(fs::canonicalize(backing.trim_end())?))
    }

    fn remount(&self, r: &Export, write: bool) -> io::Result<()> {
        command(
            Command::new("mount")
                .arg("-o")
                .arg(r.mount_options("remount", write))
                .arg(&self.share_root),
        )
    }
    fn mount(&self, r: &Export, image: &Path, write: bool) -> io::Result<()> {
        command(
            Command::new("mount")
                .arg("-o")
                .arg(r.mount_options("loop", write))
                .arg(image)
                .arg(&self.share_root),
        )
    }
    fn unmount(&self) -> io::Result<()> {
        command(Command::new("umount").arg(&self.share_root))
    }
    fn bind_share(&self, image: &Path) -> io::Result<()> {
        let slot = self.function.join("lun.2");
        fs::write(slot.join("ro"), b"0\n")?;
        fs::write(slot.join("file"), image.as_os_str().as_encoded_bytes())
    }
    fn unbind_share(&self) -> io::Result<()> {
        fs::write(self.function.join("lun.2/file"), b"\n")
    }

    fn prepare_export(&self, r: &Export) -> io::Result<()> {
        // Flush the mounted filesystem before reading any blocks from its disk.
        self.remount(r, false)?;
        let mut snapshot = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&r.snapshot)?;
        let mut original = File::open(&r.image)?;
        io::copy(&mut original, &mut snapshot)?;
        snapshot.sync_all()?;
        File::open(r.image.parent().unwrap())?.sync_all()?;
        drop(snapshot);
        drop(original);
        self.unmount()?;
        self.mount(r, &r.snapshot, false)?;
        self.bind_share(&r.image)
    }

    // This path is also used after a failed export and on daemon restart. It
    // is safe to retry after interruption between any two of these operations.
    fn restore_local(&self, r: &Export) -> io::Result<()> {
        match self.mounted_image()? {
            Some(local) if local == r.image => self.remount(r, true)?,
            Some(local) if local == r.snapshot => {
                self.unmount()?;
                self.mount(r, &r.image, true)?;
            }
            None => self.mount(r, &r.image, true)?,
            Some(_) => return Err(io::Error::other("unexpected filesystem on share mount")),
        }
        remove(&r.snapshot)?;
        File::open(r.snapshot.parent().unwrap())?.sync_all()?;
        remove(&self.share_record)?;
        File::open(self.share_record.parent().unwrap())?.sync_all()
    }

    pub(super) fn switch_share(&self, export: bool) -> io::Result<()> {
        if !export {
            let r = self
                .checkpoint()?
                .ok_or_else(|| io::Error::other("missing shared-disk recovery record"))?;
            self.unbind_share()?;
            if let Err(err) = self.restore_local(&r) {
                // For example, a still-open local download can pin the read-only
                // snapshot. Restore the preceding export only if it is safe.
                if self.mounted_image()?.as_ref() == Some(&r.snapshot) {
                    self.bind_share(&r.image)?;
                }
                return Err(err);
            }
            return Ok(());
        }
        if let Some(r) = self.checkpoint()? {
            self.restore_local(&r)?;
        }
        let image = self.share_image.canonicalize()?;
        if self.mounted_image()?.as_ref() != Some(&image) {
            return Err(io::Error::other(
                "share root must mount the configured loop image",
            ));
        }
        let root = self.share_root.canonicalize()?;
        let snapshot = image.with_file_name(format!("{TEMP_PREFIX}view-{}", Uuid::new_v4()));
        let table = fs::read_to_string("/proc/self/mountinfo")?;
        let (_, options) = mounted_device(&table, &root)?;
        let r = Export {
            options,
            image,
            root,
            snapshot,
        };
        // Write intent first. Before host attachment, recovery always restores
        // the original mount and discards an incomplete copy.
        crate::atomic_write(&self.share_record, &serde_json::to_vec(&r)?)?;
        if let Err(cause) = self.prepare_export(&r) {
            self.restore_local(&r).map_err(|repair| {
                io::Error::other(format!("{cause}; restoring share: {repair}"))
            })?;
            return Err(cause);
        }
        Ok(())
    }

    pub(super) fn recover_share(&self, bound: Option<PathBuf>) -> io::Result<bool> {
        let record = self.checkpoint()?;
        if let Some(bound) = bound {
            if bound.canonicalize()? != self.share_image.canonicalize()? {
                return Err(io::Error::other("unexpected image in shared LUN"));
            }
            if let Some(r) = &record {
                if self.mounted_image()?.as_ref() == Some(&r.snapshot)
                    && fs::read_to_string(self.function.join("lun.2/ro"))?.trim() == "0"
                {
                    return Ok(true);
                }
            }
            // Migrate the older both-sides-read-only export or repair a mount
            // removed outside kvmd. Detach before touching the live filesystem.
            self.unbind_share()?;
            if let Some(r) = &record {
                self.restore_local(r)?
            }
            self.switch_share(true)?;
            return Ok(true);
        }
        if let Some(r) = record {
            self.restore_local(&r)?
        }
        Ok(false)
    }
}
