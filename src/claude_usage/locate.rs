//! Where the `claude` executable is: override, user folders, `PATH`, fixed folders and the
//! binary shipped by the VS Code extension. All locations are injected, so it is tested with
//! temporary folders.
use std::path::{Path, PathBuf};

use super::ENV_OVERRIDE;
use crate::binary_search::{
    common_install_dirs, existing_file, first_file, home_dir, non_empty_var, path_dirs,
};

/// File names tried in every folder. On Windows the npm install leaves a `claude.cmd` shim
/// and the native installer a `claude.exe`.
#[cfg(windows)]
pub(super) const BINARY_NAMES: &[&str] = &["claude.exe", "claude.cmd"];
#[cfg(not(windows))]
pub(super) const BINARY_NAMES: &[&str] = &["claude"];

/// Where to look for `claude`. All fields injected by the caller.
#[derive(Debug, Default, Clone)]
pub struct Locations {
    pub override_path: Option<PathBuf>,
    pub home: Option<PathBuf>,
    pub path_dirs: Vec<PathBuf>,
    pub fixed_dirs: Vec<PathBuf>,
}

impl Locations {
    pub fn from_environment() -> Self {
        Locations {
            override_path: non_empty_var(ENV_OVERRIDE).map(PathBuf::from),
            home: home_dir(),
            path_dirs: path_dirs(),
            fixed_dirs: if cfg!(windows) {
                // `npm install -g` puts the shim here, and PATH is not inherited by login items.
                non_empty_var("APPDATA")
                    .map(|appdata| vec![PathBuf::from(appdata).join("npm")])
                    .unwrap_or_default()
            } else {
                common_install_dirs()
            },
        }
    }
}

/// Override, then `~/.local/bin`, `PATH`, fixed folders, and finally the native binary that the
/// VS Code extension ships (newest by folder name).
pub fn find_claude_binary(locations: &Locations) -> Option<PathBuf> {
    if let Some(path) = &locations.override_path {
        return existing_file(path);
    }
    let local = locations
        .home
        .iter()
        .map(|home| home.join(".local/bin"))
        .collect::<Vec<_>>();
    first_file(
        local
            .iter()
            .chain(&locations.path_dirs)
            .chain(&locations.fixed_dirs),
        BINARY_NAMES,
    )
    .or_else(|| locations.home.as_deref().and_then(extension_binary))
}

fn extension_binary(home: &Path) -> Option<PathBuf> {
    let extensions = std::fs::read_dir(home.join(".vscode/extensions")).ok()?;
    let mut found: Vec<PathBuf> = extensions
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("anthropic.claude-code-")
        })
        .map(|entry| {
            entry
                .path()
                .join("resources/native-binary")
                .join(BINARY_NAMES[0])
        })
        .filter(|path| path.is_file())
        .collect();
    found.sort();
    found.pop()
}
