//! Moving over from the app's previous name, "Monitor Creditos" (`monitor-creditos`): its
//! settings file and its start-at-login entry are taken over once, so renaming loses nothing.
use std::path::Path;

/// Folder, LaunchAgent and registry name used before the rename.
pub const OLD_NAME: &str = "monitor-creditos";

/// Copies `old` to `new` when `new` does not exist yet and `old` is a file. Returns whether
/// it copied. The old file stays, so the old app (if still installed) keeps working.
pub fn adopt_file(old: &Path, new: &Path) -> bool {
    if new.exists() || !old.is_file() {
        return false;
    }
    let Ok(bytes) = crate::small_file::read(old, crate::small_file::MAX_BYTES) else {
        return false;
    };
    crate::atomic_write::write(new, &bytes).is_ok()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cuantoquedapp-legacy-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_old_file_is_copied_into_a_new_folder() {
        let dir = temp_dir("copy");
        let old = dir.join(OLD_NAME).join("config.json");
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::write(&old, b"{\"theme\":\"dark\"}").unwrap();
        let new = dir.join("cuantoquedapp").join("config.json");
        assert!(adopt_file(&old, &new));
        assert_eq!(std::fs::read(&new).unwrap(), b"{\"theme\":\"dark\"}");
        assert!(old.exists(), "the old file stays");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_existing_new_file_is_never_overwritten() {
        let dir = temp_dir("keep");
        let old = dir.join("old.json");
        let new = dir.join("new.json");
        std::fs::write(&old, b"old").unwrap();
        std::fs::write(&new, b"new").unwrap();
        assert!(!adopt_file(&old, &new));
        assert_eq!(std::fs::read(&new).unwrap(), b"new");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn without_an_old_file_nothing_happens() {
        let dir = temp_dir("none");
        let new = dir.join("new").join("config.json");
        assert!(!adopt_file(&dir.join("missing.json"), &new));
        assert!(!new.exists());
        assert!(!adopt_file(&dir, &new), "a folder is not a file");
        let _ = std::fs::remove_dir_all(dir);
    }
}
