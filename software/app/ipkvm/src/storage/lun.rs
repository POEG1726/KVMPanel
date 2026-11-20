use super::Mount;
use std::{
    ffi::{OsStr, OsString},
    fs, io,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

pub(super) trait Lun: Send + Sync {
    fn bind(&self, path: Option<&Path>, mode: &str, readonly: bool) -> io::Result<()>;
    fn share(&self, exported: bool) -> io::Result<()>;
    fn restore(&self, sd: &Path) -> io::Result<(Option<Mount>, bool)>;
    fn args(&self, mounted: Option<&Mount>, sd: &Path, exported: bool)
        -> Vec<(OsString, OsString)>;
}

pub(super) struct SimLun;
impl Lun for SimLun {
    fn bind(&self, _: Option<&Path>, _: &str, _: bool) -> io::Result<()> {
        Ok(())
    }
    fn share(&self, _: bool) -> io::Result<()> {
        Ok(())
    }
    fn restore(&self, _: &Path) -> io::Result<(Option<Mount>, bool)> {
        Ok((None, false))
    }
    fn args(&self, _: Option<&Mount>, _: &Path, _: bool) -> Vec<(OsString, OsString)> {
        vec![]
    }
}

pub(super) struct BoardLun {
    pub function: PathBuf,
    pub share_root: PathBuf,
    pub share_image: PathBuf,
    pub share_record: PathBuf,
}

// mountinfo encodes whitespace and backslashes in path fields as octal escapes.
fn mount_path(field: &str) -> PathBuf {
    let mut bytes = Vec::new();
    let mut input = field.as_bytes();
    while !input.is_empty() {
        if input.len() >= 4
            && input[0] == b'\\'
            && input[1..4].iter().all(|b| (b'0'..=b'7').contains(b))
        {
            bytes.push(((input[1] - b'0') << 6) | ((input[2] - b'0') << 3) | (input[3] - b'0'));
            input = &input[4..];
        } else {
            bytes.push(input[0]);
            input = &input[1..];
        }
    }
    PathBuf::from(OsStr::from_bytes(&bytes))
}

pub(super) fn mounted_device<'a>(
    table: &'a str,
    mount: &Path,
) -> io::Result<(&'a str, Vec<String>)> {
    if mount == Path::new("/") {
        return Err(io::Error::other(
            "the root filesystem cannot be a share mount",
        ));
    }
    let mut row = None;
    for line in table.lines() {
        let Some((head, tail)) = line.split_once(" - ") else {
            continue;
        };
        let mut fields = head.split_ascii_whitespace().skip(2);
        let (Some(dev), Some(root), Some(at), Some(flags)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if mount_path(at) != mount {
            continue;
        }
        if root != "/" || row.is_some() {
            return Err(io::Error::other(
                "shared disk requires an independent mountpoint",
            ));
        }
        let super_flags = tail.split_ascii_whitespace().nth(2).unwrap_or("");
        let mut options = Vec::new();
        for opt in flags.split(',').chain(super_flags.split(',')) {
            if !matches!(opt, "" | "ro" | "rw") && !options.iter().any(|o| o == opt) {
                options.push(opt.to_owned());
            }
        }
        row = Some((dev, options));
    }
    let (dev, options) =
        row.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "share mount is absent"))?;
    if table
        .lines()
        .filter(|l| l.split_ascii_whitespace().nth(2) == Some(dev))
        .count()
        != 1
    {
        return Err(io::Error::other(
            "shared loop disk has multiple local mounts",
        ));
    }
    Ok((dev, options))
}
impl Lun for BoardLun {
    fn bind(&self, path: Option<&Path>, mode: &str, readonly: bool) -> io::Result<()> {
        let lun = self
            .function
            .join(if mode == "cdrom" { "lun.0" } else { "lun.1" });
        if let Some(file) = path {
            fs::write(lun.join("ro"), if readonly { "1\n" } else { "0\n" })?;
            fs::write(lun.join("file"), file.as_os_str().as_encoded_bytes())
        } else {
            fs::write(lun.join("file"), b"\n")
        }
    }

    fn share(&self, exported: bool) -> io::Result<()> {
        self.switch_share(exported)
    }

    fn restore(&self, sd: &Path) -> io::Result<(Option<Mount>, bool)> {
        let read_path = |slot: &str| -> io::Result<Option<PathBuf>> {
            match fs::read_to_string(self.function.join(slot).join("file")) {
                Ok(s) => Ok(if s.trim_end().is_empty() {
                    None
                } else {
                    Some(s.trim_end().into())
                }),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e),
            }
        };
        let mut mount = None;
        for (slot, mode) in [("lun.0", "cdrom"), ("lun.1", "disk")] {
            if let Some(p) = read_path(slot)? {
                let rel = p
                    .strip_prefix(sd)
                    .map_err(|_| io::Error::other("mounted image outside configured SD root"))?;
                if mount.is_some() {
                    return Err(io::Error::other("two virtual images already mounted"));
                }
                mount = Some(Mount {
                    path: rel.into(),
                    mode: mode.into(),
                    readonly: fs::read_to_string(self.function.join(slot).join("ro"))?.trim()
                        == "1",
                });
            }
        }
        let exported = self.recover_share(read_path("lun.2")?)?;
        Ok((mount, exported))
    }

    fn args(
        &self,
        mounted: Option<&Mount>,
        sd: &Path,
        exported: bool,
    ) -> Vec<(OsString, OsString)> {
        let mut args: Vec<(OsString, OsString)> = vec![("SHARE_RO".into(), "0".into())];
        if let Some(m) = mounted {
            args.push((
                if m.mode == "disk" {
                    "DISK_IMAGE"
                } else {
                    "CDROM_IMAGE"
                }
                .into(),
                sd.join(&m.path).into(),
            ));
            args.push(("DISK_RO".into(), if m.readonly { "1" } else { "0" }.into()));
        }
        if exported {
            args.push(("SHARE_IMAGE".into(), self.share_image.clone().into()));
        }
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn share_requires_exact_mountpoint() {
        let table="26 25 8:1 / / rw - ext4 /dev/sda1 rw\n47 26 7:3 / /mnt/share\\040dir rw,noexec,nodev - vfat /dev/loop3 rw,uid=1000\n";
        assert_eq!(
            mounted_device(table, Path::new("/mnt/share dir")).unwrap(),
            (
                "7:3",
                vec![
                    "noexec".to_owned(),
                    "nodev".to_owned(),
                    "uid=1000".to_owned()
                ]
            )
        );
        assert!(mounted_device(table, Path::new("/mnt/share dir/sub")).is_err());
        assert!(mounted_device(table, Path::new("/")).is_err());
    }
    #[test]
    fn board_binds_restores_and_ejects_disk_on_fixture() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = BoardLun {
            function: dir.path().join("storage"),
            share_image: dir.path().join("share.img"),
            share_record: dir.path().join("share-export.json"),
            share_root: dir.path().join("share"),
        };
        let sd = dir.path().join("sd");
        let image = sd.join("one.img");
        for num in 0..3 {
            let lun = cfg.function.join(format!("lun.{num}"));
            fs::create_dir_all(&lun).unwrap();
            fs::write(lun.join("ro"), "1").unwrap();
            fs::write(lun.join("file"), "").unwrap();
        }
        cfg.bind(Some(&image), "disk", false).unwrap();
        let (mounted, exported) = cfg.restore(&sd).unwrap();
        let m = mounted.as_ref().unwrap();
        assert_eq!(m.path, Path::new("one.img"));
        assert_eq!(m.mode, "disk");
        assert!(!m.readonly);
        assert!(!exported);
        assert!(cfg
            .args(mounted.as_ref(), &sd, false)
            .contains(&("DISK_IMAGE".into(), image.into())));
        let shared = cfg.args(Some(m), &sd, true);
        assert!(shared.contains(&("SHARE_RO".into(), "0".into())));
        assert!(shared.contains(&("SHARE_IMAGE".into(), cfg.share_image.clone().into())));
        assert!(!cfg
            .args(None, &sd, false)
            .iter()
            .any(|(key, _)| key == "SHARE_IMAGE"));
        cfg.bind(None, "disk", true).unwrap();
        assert!(cfg.restore(&sd).unwrap().0.is_none());
    }
}
