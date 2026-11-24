//! Single running instance of the tray app, using an OS file lock.
//!
//! The lock lives as long as the guard (and the process): the OS releases it when the
//! process exits or crashes, so there is no stale PID file to clean up.
use std::{
    ffi::OsString,
    fs::{File, OpenOptions, TryLockError},
    path::{Path, PathBuf},
};

/// Holds the lock; dropping it (or exiting the process) releases it.
#[derive(Debug)]
pub struct InstanceGuard {
    _file: File,
}

#[derive(Debug)]
pub enum Acquire {
    Acquired(InstanceGuard),
    /// Another copy of the app already holds the lock.
    AlreadyRunning,
}

/// Fixed per-user path. Not the temp dir: a login item may start without `TMPDIR`
/// and fall back to `/tmp`, so two copies would lock different files.
/// macOS: `~/Library/Caches/cuantoquedapp/instance.lock`;
/// Windows: `%LOCALAPPDATA%\cuantoquedapp\instance.lock`.
pub fn default_lock_path() -> Option<PathBuf> {
    lock_path_from(|name| std::env::var_os(name))
}

/// Environment injected, to test it without touching real variables.
fn lock_path_from(env: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let non_empty = |name: &str| env(name).filter(|value| !value.is_empty());
    let base = match non_empty("LOCALAPPDATA") {
        Some(local) => PathBuf::from(local),
        None => PathBuf::from(non_empty("HOME")?)
            .join("Library")
            .join("Caches"),
    };
    Some(base.join("cuantoquedapp").join("instance.lock"))
}

/// Tries to take the lock at `path`, creating its folder if needed.
/// `Err` means the lock could not be tried at all (not that another copy runs).
pub fn acquire(path: &Path) -> Result<Acquire, std::io::Error> {
    if let Some(parent) = path.parent() {
        crate::atomic_write::private_folder(parent)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Acquire::Acquired(InstanceGuard { _file: file })),
        Err(TryLockError::WouldBlock) => Ok(Acquire::AlreadyRunning),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

/// `acquire` at the default path.
pub fn acquire_default() -> Result<Acquire, std::io::Error> {
    let path = default_lock_path().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no user folder found for the lock file",
        )
    })?;
    acquire(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_lock(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("cuantoquedapp-lock-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder.join("nested").join("instance.lock")
    }

    fn env(pairs: &[(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
        let pairs = pairs.to_vec();
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    #[test]
    fn the_first_copy_gets_the_lock_and_creates_its_folder() {
        let path = temp_lock("first");
        assert!(matches!(acquire(&path), Ok(Acquire::Acquired(_))));
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn a_second_copy_is_told_another_one_is_running() {
        let path = temp_lock("second");
        let first = acquire(&path).unwrap();
        assert!(matches!(first, Acquire::Acquired(_)));
        assert!(matches!(acquire(&path), Ok(Acquire::AlreadyRunning)));
        assert!(matches!(acquire(&path), Ok(Acquire::AlreadyRunning)));
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn the_lock_is_released_when_the_guard_is_dropped() {
        let path = temp_lock("release");
        let first = acquire(&path).unwrap();
        drop(first);
        // Another test may be spawning a process at this very moment, and for an instant the
        // new child holds a copy of the file descriptor: give the lock a moment to be free.
        let released = (0..50).any(|_| match acquire(&path) {
            Ok(Acquire::Acquired(_)) => true,
            _ => {
                std::thread::sleep(std::time::Duration::from_millis(20));
                false
            }
        });
        assert!(released);
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn different_paths_do_not_block_each_other() {
        let a = temp_lock("a");
        let b = temp_lock("b");
        let _first = acquire(&a).unwrap();
        assert!(matches!(acquire(&b), Ok(Acquire::Acquired(_))));
        for path in [a, b] {
            let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
        }
    }

    #[test]
    fn windows_uses_localappdata() {
        let path = lock_path_from(env(&[
            ("LOCALAPPDATA", "C:/Users/x/AppData/Local"),
            ("HOME", "/ignored"),
        ]));
        assert_eq!(
            path.unwrap(),
            PathBuf::from("C:/Users/x/AppData/Local/cuantoquedapp/instance.lock")
        );
    }

    #[test]
    fn mac_uses_library_caches_under_home() {
        let path = lock_path_from(env(&[("HOME", "/Users/x")]));
        assert_eq!(
            path.unwrap(),
            PathBuf::from("/Users/x/Library/Caches/cuantoquedapp/instance.lock")
        );
    }

    #[test]
    fn without_a_user_folder_there_is_no_path() {
        assert!(lock_path_from(env(&[])).is_none());
        assert!(lock_path_from(env(&[("HOME", ""), ("LOCALAPPDATA", "")])).is_none());
    }
}
