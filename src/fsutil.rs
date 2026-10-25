//! File helpers shared by every writer: atomic replacement that keeps
//! symlinks and permissions, daily config backups, and advisory locks.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
#[cfg(unix)]
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
            mode = permissions(&meta).unwrap_or(mode);
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp = temp_path(dir, ".herdr-customize-");
    let result = (|| -> Result<()> {
        let mut f = with_mode(OpenOptions::new().write(true).create_new(true), mode).open(&tmp)?;
        #[cfg(unix)]
        f.set_permissions(fs::Permissions::from_mode(mode))?;
        f.write_all(data)?;
        f.sync_all()?;
        // Replaces an existing file on every platform (MoveFileEx on Windows).
        fs::rename(&tmp, &path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Atomically replaces path after keeping the day's backup of what it held.
/// A missing file gets its directory created and no backup.
pub fn write_backed_up(path: &Path, data: &[u8], mode: u32) -> Result<()> {
    match fs::read(path) {
        Ok(old) => daily_backup(path, &old)?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => fs::create_dir_all(path.parent().unwrap_or_else(|| Path::new(".")))?,
        Err(e) => return Err(e.into()),
    }
    atomic_write(path, data, mode)
}

/// Keeps one copy of the original file per day as path.bak-YYYYMMDD. An
/// existing backup for today is never overwritten.
fn daily_backup(path: &Path, data: &[u8]) -> Result<()> {
    let mut bak = path.as_os_str().to_owned();
    bak.push(format!(".bak-{}", today()));
    let bak = PathBuf::from(bak);
    match with_mode(OpenOptions::new().write(true).create_new(true), 0o600).open(&bak) {
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
    let f = with_mode(OpenOptions::new().create(true).truncate(false).read(true).write(true), 0o600).open(path)?;
    f.lock()?;
    Ok(f)
}

/// Holds the write lock for path, kept in the plugin's state folder, until
/// the returned file is dropped. Writers in this and every other process
/// take turns, so none loads the file before another has replaced it.
pub fn write_lock(path: &Path) -> Result<File> {
    let dir = crate::theme::state_dir().join("locks");
    fs::create_dir_all(&dir)?;
    lock(&dir.join(format!("{:016x}.lock", path_hash(path)))).with_context(|| format!("lock {}", path.display()))
}

/// FNV-1a of the real path. Unlike std's hasher it never changes between
/// builds, so every version of the plugin picks the same lock.
fn path_hash(path: &Path) -> u64 {
    let mut text = real_path(path).to_string_lossy().into_owned();
    if cfg!(windows) {
        text = text.to_lowercase();
    }
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

/// path with symlinks and junctions resolved, so every way to reach a file
/// names the same one. A missing file resolves its closest existing parent.
fn real_path(path: &Path) -> PathBuf {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut missing = Vec::new();
    let mut base = abs.as_path();
    loop {
        if let Ok(real) = fs::canonicalize(base) {
            return missing.iter().rev().fold(real, |p, name| p.join(name));
        }
        match (base.parent(), base.file_name()) {
            (Some(parent), Some(name)) => {
                missing.push(name.to_os_string());
                base = parent;
            }
            _ => return abs,
        }
    }
}

/// Unix permission bits for new files.
#[cfg(unix)]
fn with_mode(o: &mut OpenOptions, mode: u32) -> &mut OpenOptions {
    o.mode(mode)
}

/// Windows files inherit their ACLs instead.
#[cfg(windows)]
fn with_mode(o: &mut OpenOptions, _: u32) -> &mut OpenOptions {
    o
}

#[cfg(unix)]
fn permissions(meta: &fs::Metadata) -> Option<u32> {
    Some(meta.permissions().mode() & 0o777)
}

#[cfg(windows)]
fn permissions(_: &fs::Metadata) -> Option<u32> {
    None
}

/// A fresh path in dir that no other writer uses.
pub fn temp_path(dir: &Path, prefix: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    dir.join(format!("{prefix}{}-{n}-{nanos}", std::process::id()))
}

/// Today's local date as YYYYMMDD.
fn today() -> String {
    format_local(std::time::SystemTime::now(), "%Y%m%d")
}

/// Formats t in local time, replacing %Y, %m, %d, %H, %M and %S in fmt.
pub fn format_local(t: std::time::SystemTime, fmt: &str) -> String {
    let t = local_time(t);
    let fields = [
        ("%Y", format!("{:04}", t.year)),
        ("%m", format!("{:02}", t.month)),
        ("%d", format!("{:02}", t.day)),
        ("%H", format!("{:02}", t.hour)),
        ("%M", format!("{:02}", t.minute)),
        ("%S", format!("{:02}", t.second)),
    ];
    fields.iter().fold(fmt.to_string(), |out, (code, value)| out.replace(code, value))
}

struct LocalTime {
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
}

fn unix_seconds(t: std::time::SystemTime) -> i64 {
    t.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// Breaks t into local calendar fields.
#[cfg(unix)]
fn local_time(t: std::time::SystemTime) -> LocalTime {
    // Naming libc::time_t warns on musl, where it is due to become 64-bit on
    // 32-bit targets; let localtime_r's signature pick the type.
    #[allow(clippy::useless_conversion, reason = "time_t is 32-bit on some targets")]
    let secs = unix_seconds(t).try_into().unwrap_or_default();
    // SAFETY: localtime_r only writes the tm we pass it.
    let tm = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&secs, &mut tm);
        tm
    };
    LocalTime { year: tm.tm_year + 1900, month: tm.tm_mon as u32 + 1, day: tm.tm_mday as u32, hour: tm.tm_hour as u32, minute: tm.tm_min as u32, second: tm.tm_sec as u32 }
}

/// Breaks t into local calendar fields.
#[cfg(windows)]
fn local_time(t: std::time::SystemTime) -> LocalTime {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
    // FILETIME counts 100 ns intervals from 1601-01-01.
    let ticks = (unix_seconds(t) as u64 + 11_644_473_600) * 10_000_000;
    let ft = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    // SAFETY: both calls only write the SYSTEMTIME values we pass them.
    let st = unsafe {
        let mut utc: SYSTEMTIME = std::mem::zeroed();
        let mut local: SYSTEMTIME = std::mem::zeroed();
        FileTimeToSystemTime(&ft, &mut utc);
        if SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) == 0 {
            local = utc;
        }
        local
    };
    LocalTime { year: st.wYear as i32, month: st.wMonth as u32, day: st.wDay as u32, hour: st.wHour as u32, minute: st.wMinute as u32, second: st.wSecond as u32 }
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

    #[cfg(unix)]
    #[test]
    fn atomic_write_keeps_symlink_and_mode() {
        use std::os::unix::fs::PermissionsExt;
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
    fn format_local_fills_every_field() {
        let text = format_local(std::time::SystemTime::now(), "on %Y-%m-%d at %H:%M:%S");
        assert!(regex::Regex::new(r"^on \d{4}-\d{2}-\d{2} at \d{2}:\d{2}:\d{2}$").unwrap().is_match(&text), "{text}");
    }

    // Every process and plugin version must name the same lock for a file.
    #[test]
    fn path_hash_is_stable() {
        assert_eq!(path_hash(Path::new("/herdr/config.toml")), path_hash(Path::new("/herdr/config.toml")));
        assert_ne!(path_hash(Path::new("/herdr/config.toml")), path_hash(Path::new("/herdr/other.toml")));
    }

    // A config reached through a linked folder must share the original's lock,
    // whether or not the file exists yet.
    #[test]
    fn linked_paths_share_a_lock() {
        let dir = tempdir("linked");
        let real = dir.join("real");
        fs::create_dir_all(&real).unwrap();
        let link = dir.join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &link).unwrap();
        // A junction needs no privilege, unlike a Windows symlink.
        #[cfg(windows)]
        assert!(std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(&link).arg(&real).output().unwrap().status.success());
        assert_eq!(path_hash(&link.join("config.toml")), path_hash(&real.join("config.toml")));
        fs::write(real.join("config.toml"), "").unwrap();
        assert_eq!(path_hash(&link.join("config.toml")), path_hash(&real.join("config.toml")));
    }

    #[test]
    fn daily_backup_keeps_the_first_original() {
        let dir = tempdir("bak").join("new");
        let path = dir.join("config.toml");
        write_backed_up(&path, b"first", 0o600).unwrap();
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "a new file has nothing to back up");
        write_backed_up(&path, b"second", 0o600).unwrap();
        write_backed_up(&path, b"third", 0o600).unwrap();
        let bak = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).find(|p| p != &path).unwrap();
        assert_eq!(fs::read(bak).unwrap(), b"first");
        assert_eq!(fs::read(&path).unwrap(), b"third");
    }
}
