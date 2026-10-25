//! File helpers shared by every writer: atomic replacement that keeps
//! symlinks and permissions, daily config backups, and advisory locks.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// Writes data so readers never observe a truncated file. An existing
/// symlink is resolved, so updating a linked config preserves the link, and
/// an existing file keeps its permissions.
pub fn atomic_write(path: &Path, data: &[u8], mode: u32) -> Result<()> {
    let mut path = path.to_path_buf();
    let mut mode = mode;
    match fs::symlink_metadata(&path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                path = fs::canonicalize(&path)?;
            }
            let meta = fs::metadata(&path)?;
            if !meta.is_file() {
                bail!("not a regular file: {}", path.display());
            }
            mode = meta.permissions().mode() & 0o777;
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let dir = path.parent().unwrap_or(Path::new("."));
    let tmp = temp_path(dir, ".herdr-customize-");
    let result = (|| -> Result<()> {
        let mut f = OpenOptions::new().write(true).create_new(true).mode(mode).open(&tmp)?;
        f.set_permissions(fs::Permissions::from_mode(mode))?;
        f.write_all(data)?;
        f.sync_all()?;
        fs::rename(&tmp, &path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Keeps one copy of the original file per day as path.bak-YYYYMMDD. An
/// existing backup for today is never overwritten.
pub fn daily_backup(path: &Path, data: &[u8]) -> Result<()> {
    let mut bak = path.as_os_str().to_owned();
    bak.push(format!(".bak-{}", today()));
    let bak = PathBuf::from(bak);
    match OpenOptions::new().write(true).create_new(true).mode(0o600).open(&bak) {
        Ok(mut f) => {
            if let Err(e) = f.write_all(data).and_then(|_| f.sync_all()) {
                let _ = fs::remove_file(&bak);
                return Err(e).context("config backup failed");
            }
            Ok(())
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            if !fs::symlink_metadata(&bak).context("check config backup")?.is_file() {
                bail!("config backup is not a regular file: {}", bak.display());
            }
            Ok(())
        }
        Err(e) => Err(e).context("config backup failed"),
    }
}

/// Holds path's advisory lock until the returned file is dropped. The lock
/// file stays on disk so every process locks the same file.
pub fn lock(path: &Path) -> Result<File> {
    let f = OpenOptions::new().create(true).truncate(false).read(true).write(true).mode(0o600).open(path)?;
    f.lock()?;
    Ok(f)
}

/// A fresh path in dir that no other writer uses.
pub fn temp_path(dir: &Path, prefix: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    dir.join(format!("{prefix}{}-{n}-{nanos}", std::process::id()))
}

/// Today's local date as YYYYMMDD.
fn today() -> String {
    let tm = local_time(std::time::SystemTime::now());
    format!("{:04}{:02}{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday)
}

/// Breaks t into local calendar fields.
pub fn local_time(t: std::time::SystemTime) -> libc::tm {
    let secs = match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as libc::time_t,
        Err(_) => 0,
    };
    // SAFETY: localtime_r only writes the tm we pass it.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&secs, &mut tm);
        tm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hc-fsutil-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn atomic_write_keeps_symlink_and_mode() {
        let dir = tempdir("link");
        let real = dir.join("real.toml");
        fs::write(&real, "a = 1\n").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o640)).unwrap();
        let link = dir.join("config.toml");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        atomic_write(&link, b"a = 2\n", 0o600).unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), "a = 2\n");
        assert_eq!(fs::metadata(&real).unwrap().permissions().mode() & 0o777, 0o640);
    }

    #[test]
    fn daily_backup_never_overwrites() {
        let dir = tempdir("bak");
        let path = dir.join("config.toml");
        daily_backup(&path, b"first").unwrap();
        daily_backup(&path, b"second").unwrap();
        let bak = fs::read_dir(&dir).unwrap().next().unwrap().unwrap().path();
        assert_eq!(fs::read(bak).unwrap(), b"first");
    }
}
