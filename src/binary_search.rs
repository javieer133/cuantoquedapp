//! What both executable finders (`codex_binary`, `claude_usage`) need from the environment.
//! Kept apart so they only differ in what they look for, not in how they read the system.
//!
//! Only absolute paths are ever accepted. A relative one depends on the current folder, and
//! the children run in another one (Claude in the temporary folder): the file checked here
//! would not be the file executed.
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// An environment variable that is set and not empty.
pub fn non_empty_var(name: &str) -> Option<OsString> {
    std::env::var_os(name).filter(|value| !value.is_empty())
}

/// A variable holding an absolute path; a relative one is ignored (see the module docs).
pub fn absolute_var(name: &str) -> Option<PathBuf> {
    non_empty_var(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

/// The user's folder (`HOME`, or `USERPROFILE` on Windows), if absolute.
pub fn home_dir() -> Option<PathBuf> {
    absolute_var("HOME").or_else(|| absolute_var("USERPROFILE"))
}

/// The absolute folders of `PATH` (relative or empty entries are skipped). A GUI app opened
/// from the Dock or at login may not have the shell's `PATH`, which is why the finders also
/// look in well-known places.
pub fn path_dirs() -> Vec<PathBuf> {
    non_empty_var("PATH")
        .map(|path| absolute_only(std::env::split_paths(&path)))
        .unwrap_or_default()
}

fn absolute_only(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    paths
        .into_iter()
        .filter(|path| path.is_absolute())
        .collect()
}

/// Where Homebrew and manual installs put executables (none on Windows).
pub fn common_install_dirs() -> Vec<PathBuf> {
    if cfg!(windows) {
        Vec::new()
    } else {
        vec![
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
        ]
    }
}

/// The first existing file `<dir>/<name>`, folders in order and, in each, names in order.
/// Relative folders are skipped.
pub fn first_file<'a>(
    dirs: impl IntoIterator<Item = &'a PathBuf>,
    names: &[&str],
) -> Option<PathBuf> {
    dirs.into_iter()
        .filter(|dir| dir.is_absolute())
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|candidate| candidate.is_file())
}

/// A path, or `None` if it is relative or not an existing file (used for explicit overrides).
pub fn existing_file(path: &Path) -> Option<PathBuf> {
    (path.is_absolute() && path.is_file()).then(|| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cuantoquedapp-binsearch-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn first_file_respects_folder_then_name_order_and_skips_directories() {
        let root = temp_dir("first");
        let (a, b) = (root.join("a"), root.join("b"));
        std::fs::create_dir_all(a.join("tool")).unwrap(); // a directory named like the tool
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(b.join("tool.cmd"), b"").unwrap();
        std::fs::write(b.join("tool.exe"), b"").unwrap();
        let dirs = vec![root.join("missing"), a, b.clone()];
        assert_eq!(
            first_file(&dirs, &["tool.exe", "tool.cmd"]),
            Some(b.join("tool.exe"))
        );
        assert_eq!(
            first_file(&dirs, &["tool.cmd", "tool.exe"]),
            Some(b.join("tool.cmd"))
        );
        assert_eq!(
            first_file(&dirs, &["tool"]),
            None,
            "a directory is not an executable"
        );
        assert_eq!(first_file(&[], &["tool"]), None);
        let _ = std::fs::remove_dir_all(root);
    }

    /// A folder relative to the current one (the crate root under `cargo test`).
    struct RelativeDir(PathBuf);

    impl RelativeDir {
        fn new(name: &str) -> Self {
            let dir = PathBuf::from(format!(
                "target/cuantoquedapp-relative-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("claude"), b"").unwrap();
            RelativeDir(dir)
        }
    }

    impl Drop for RelativeDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    // SEC-01: the file checked must be the file executed, whatever the child's folder.
    #[test]
    fn relative_folders_and_overrides_are_never_chosen() {
        let relative = RelativeDir::new("lookup");
        assert!(relative.0.join("claude").is_file(), "exists from here");
        assert_eq!(first_file([&relative.0], &["claude"]), None);
        assert_eq!(existing_file(&relative.0.join("claude")), None);
        let absolute = std::env::current_dir().unwrap().join(&relative.0);
        assert_eq!(
            first_file([&relative.0, &absolute], &["claude"]),
            Some(absolute.join("claude")),
            "a relative folder is skipped, not a dead end"
        );
        assert_eq!(
            absolute_only([PathBuf::from("bin"), PathBuf::from(""), absolute.clone()]),
            [absolute]
        );
    }

    #[test]
    fn existing_file_only_accepts_files() {
        let root = temp_dir("existing");
        let file = root.join("f");
        std::fs::write(&file, b"").unwrap();
        assert_eq!(existing_file(&file), Some(file));
        assert_eq!(existing_file(&root), None);
        assert_eq!(existing_file(&root.join("nope")), None);
        let _ = std::fs::remove_dir_all(root);
    }
}
