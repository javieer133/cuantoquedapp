//! Finds the native `codex` executable. An app opened from Applications (or at login) does not
//! inherit the shell `PATH`, and the `codex` in a terminal is often a Node script, so the
//! search looks first at the native binary that Codex itself downloads under its home folder.
//!
//! Pure lookup over injected locations (no environment access), so tests use temp folders.
use std::path::{Path, PathBuf};

use crate::binary_search::{
    absolute_var, common_install_dirs, existing_file, first_file, home_dir, non_empty_var,
    path_dirs,
};

/// Environment variable that points straight to the executable (highest priority).
pub const ENV_OVERRIDE: &str = "CUANTOQUEDAPP_CODEX";

#[cfg(windows)]
const BINARY_NAME: &str = "codex.exe";
#[cfg(not(windows))]
const BINARY_NAME: &str = "codex";

/// Where to look. All fields are injected by the caller.
#[derive(Debug, Default, Clone)]
pub struct Locations {
    /// Value of `CUANTOQUEDAPP_CODEX`, if set.
    pub override_path: Option<PathBuf>,
    /// Codex home (`CODEX_HOME` or `~/.codex`).
    pub codex_home: Option<PathBuf>,
    /// Directories of the `PATH` variable.
    pub path_dirs: Vec<PathBuf>,
    /// Well-known install folders checked last.
    pub fixed_dirs: Vec<PathBuf>,
}

impl Locations {
    /// Builds the locations from the real environment.
    pub fn from_environment() -> Self {
        // Only absolute paths (see `binary_search`): a relative one is ignored.
        let codex_home =
            absolute_var("CODEX_HOME").or_else(|| home_dir().map(|home| home.join(".codex")));
        Locations {
            override_path: non_empty_var(ENV_OVERRIDE).map(PathBuf::from),
            codex_home,
            path_dirs: path_dirs(),
            fixed_dirs: common_install_dirs(),
        }
    }
}

/// First existing executable, in this order: override, native release under the Codex home
/// (newest version first), `PATH`, fixed folders. `None` means live data is unavailable.
pub fn find_codex_binary(locations: &Locations) -> Option<PathBuf> {
    if let Some(path) = &locations.override_path {
        // An explicit override that does not exist is not silently replaced by another binary.
        return existing_file(path);
    }
    if let Some(home) = &locations.codex_home
        && let Some(path) = newest_release(home)
    {
        return Some(path);
    }
    first_file(
        locations.path_dirs.iter().chain(&locations.fixed_dirs),
        &[BINARY_NAME],
    )
}

/// `<home>/packages/app-server-daemon/releases/<version>-<target>/bin/codex`, highest version.
fn newest_release(codex_home: &Path) -> Option<PathBuf> {
    let releases = codex_home.join("packages/app-server-daemon/releases");
    let entries = std::fs::read_dir(releases).ok()?;
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let version = parse_version(&entry.file_name().to_string_lossy())?;
            let binary = entry.path().join("bin").join(BINARY_NAME);
            binary.is_file().then_some((version, binary))
        })
        .max_by_key(|(version, _)| *version)
        .map(|(_, binary)| binary)
}

/// Leading `major.minor.patch` of a folder name like `0.171.0-aarch64-apple-darwin`.
fn parse_version(name: &str) -> Option<(u64, u64, u64)> {
    let mut parts = name.split(['.', '-']);
    let mut next = || parts.next()?.parse::<u64>().ok();
    Some((next()?, next()?, next()?))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("cuantoquedapp-bin-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"").unwrap();
    }

    fn release(home: &Path, folder: &str) -> PathBuf {
        let binary = home
            .join("packages/app-server-daemon/releases")
            .join(folder)
            .join("bin")
            .join(BINARY_NAME);
        touch(&binary);
        binary
    }

    #[test]
    fn parses_versions() {
        assert_eq!(
            parse_version("0.171.0-aarch64-apple-darwin"),
            Some((0, 171, 0))
        );
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("latest"), None);
        assert_eq!(parse_version("0.171"), None);
    }

    #[test]
    fn picks_the_newest_release_numerically() {
        let home = temp_dir("newest");
        release(&home, "0.9.0-x");
        let newest = release(&home, "0.171.0-x");
        release(&home, "0.20.5-x");
        let locations = Locations {
            codex_home: Some(home.clone()),
            ..Locations::default()
        };
        assert_eq!(find_codex_binary(&locations), Some(newest));
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn skips_releases_without_a_binary_and_odd_folders() {
        let home = temp_dir("skips");
        let releases = home.join("packages/app-server-daemon/releases");
        fs::create_dir_all(releases.join("9.9.9-empty/bin")).unwrap();
        fs::create_dir_all(releases.join("current")).unwrap();
        let good = release(&home, "0.1.0-x");
        let locations = Locations {
            codex_home: Some(home.clone()),
            ..Locations::default()
        };
        assert_eq!(find_codex_binary(&locations), Some(good));
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn override_wins_and_is_not_replaced_when_missing() {
        let home = temp_dir("override");
        release(&home, "0.1.0-x");
        let custom = home.join("custom-codex");
        touch(&custom);
        let mut locations = Locations {
            override_path: Some(custom.clone()),
            codex_home: Some(home.clone()),
            ..Locations::default()
        };
        assert_eq!(find_codex_binary(&locations), Some(custom));
        locations.override_path = Some(home.join("missing"));
        assert_eq!(find_codex_binary(&locations), None);
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn falls_back_to_path_then_fixed_dirs() {
        let root = temp_dir("path");
        let in_path = root.join("a").join(BINARY_NAME);
        let fixed = root.join("b").join(BINARY_NAME);
        touch(&in_path);
        touch(&fixed);
        let mut locations = Locations {
            path_dirs: vec![root.join("missing"), root.join("a")],
            fixed_dirs: vec![root.join("b")],
            ..Locations::default()
        };
        assert_eq!(find_codex_binary(&locations), Some(in_path));
        locations.path_dirs.clear();
        assert_eq!(find_codex_binary(&locations), Some(fixed));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn nothing_found_is_none() {
        assert_eq!(find_codex_binary(&Locations::default()), None);
    }
}
