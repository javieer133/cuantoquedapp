//! Writes a file so that readers never see half of it and two writers never share a temporary
//! file: each call writes its own uniquely named temporary file next to the target and
//! renames it over the target.
//!
//! The app's folders are private: created for the user only (0700 on Unix; on Windows they
//! live under the user's own profile), never written through a link, and the files are 0600.
use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_path_for(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let unique = format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    target.with_file_name(unique)
}

/// Creates the app's own `folder` if needed and makes sure it can be written to: it must be
/// a real folder, not a link that would send the files somewhere else. On Unix it is also
/// made private (0700), which tightens folders made by older versions.
pub fn private_folder(folder: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(folder)?;
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(folder)?;
    // Checked before changing anything on it: permissions would follow the link.
    if std::fs::symlink_metadata(folder)?.file_type().is_symlink() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "the app folder is a link; nothing is written through it",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Options for a new file only the user can read (0600 on Unix).
pub fn private_file_options() -> OpenOptions {
    #[allow(unused_mut)]
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

/// Creates the folder if needed (see `private_folder`). The temporary file is created
/// exclusively (it is never an existing file or a link) and removed if anything fails.
pub fn write(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(folder) = target.parent() {
        private_folder(folder)?;
    }
    let temp = temp_path_for(target);
    let result = (|| {
        let mut file = private_file_options()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, target)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "cuantoquedapp-atomic-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn writes_creates_the_folder_and_replaces_the_content() {
        let dir = folder("basic");
        let target = dir.join("deep/file.json");
        write(&target, b"one").unwrap();
        write(&target, b"two").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"two");
        assert_eq!(
            std::fs::read_dir(target.parent().unwrap()).unwrap().count(),
            1
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    // SEC-02: a linked app folder would send the file elsewhere; nothing is written.
    #[cfg(unix)]
    #[test]
    fn a_linked_app_folder_is_refused_and_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = folder("private");
        let elsewhere = dir.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let linked = dir.join("app");
        std::os::unix::fs::symlink(&elsewhere, &linked).unwrap();
        let error = write(&linked.join("config.json"), b"{}").unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);

        let real = dir.join("real");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o755)).unwrap();
        let target = real.join("config.json");
        write(&target, b"{}").unwrap();
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&real), 0o700, "an older, open folder is tightened");
        assert_eq!(mode(&target), 0o600);
        let _ = std::fs::remove_dir_all(dir);
    }

    // A final link is replaced by the new file; what it pointed to is left alone.
    #[cfg(unix)]
    #[test]
    fn a_linked_target_is_replaced_not_followed() {
        let dir = folder("final-link");
        std::fs::create_dir_all(&dir).unwrap();
        let outside = dir.join("outside.txt");
        std::fs::write(&outside, b"keep").unwrap();
        let target = dir.join("config.json");
        std::os::unix::fs::symlink(&outside, &target).unwrap();
        write(&target, b"new").unwrap();
        assert_eq!(std::fs::read(&outside).unwrap(), b"keep");
        assert!(
            !std::fs::symlink_metadata(&target)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    // A1
    #[test]
    fn concurrent_writers_leave_one_complete_file_and_no_leftovers() {
        let dir = folder("concurrent");
        let target = dir.join("claude-limits.json");
        let handles: Vec<_> = (0..8)
            .map(|writer| {
                let target = target.clone();
                std::thread::spawn(move || {
                    for round in 0..50 {
                        let body = format!(
                            "{{\"writer\":{writer},\"round\":{round},\"pad\":\"{}\"}}",
                            "x".repeat(2000)
                        );
                        write(&target, body.as_bytes()).unwrap();
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        let text = std::fs::read_to_string(&target).unwrap();
        assert!(
            text.starts_with("{\"writer\":") && text.ends_with("\"}"),
            "complete document"
        );
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["claude-limits.json"], "no temporary files left");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_write_leaves_no_temporary_file() {
        let dir = folder("fail");
        std::fs::create_dir_all(&dir).unwrap();
        // The target is a non-empty folder: the rename fails.
        let target = dir.join("target");
        std::fs::create_dir_all(target.join("inside")).unwrap();
        assert!(write(&target, b"data").is_err());
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["target"]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
