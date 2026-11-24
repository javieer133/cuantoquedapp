//! "Start at login": a LaunchAgent on macOS, the per-user `Run` registry key on Windows.
//!
//! Only offered for an installed app: registering `target/debug/...` would leave a login
//! item pointing at a build folder. The state is always read from the system (no config file).
use std::path::Path;

use auto_launch::{AutoLaunch, AutoLaunchBuilder, MacOSLaunchMode, WindowsEnableMode};

/// Name of the LaunchAgent (`~/Library/LaunchAgents/<name>.plist`) and of the registry value.
pub const APP_NAME: &str = "cuantoquedapp";

#[derive(Debug, PartialEq, Eq)]
pub enum Eligibility {
    Allowed,
    /// Running from Cargo's `target/debug|release`: not an installed app.
    DevelopmentBuild,
    /// The path has characters the LaunchAgent plist cannot hold (the crate does not escape XML).
    UnsafePath,
}

pub fn eligibility(exe: &Path) -> Eligibility {
    let parts: Vec<String> = exe
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    let in_cargo_target = parts
        .iter()
        .position(|part| part == "target")
        .is_some_and(|at| {
            parts[at + 1..]
                .iter()
                .any(|p| p == "debug" || p == "release")
        });
    if in_cargo_target {
        return Eligibility::DevelopmentBuild;
    }
    if exe.to_string_lossy().contains(['&', '<', '>']) {
        return Eligibility::UnsafePath;
    }
    Eligibility::Allowed
}

pub struct Autostart {
    inner: AutoLaunch,
}

impl Autostart {
    /// Handle for starting `exe` at login under the given name.
    pub fn new(app_name: &str, exe: &Path) -> Result<Self, String> {
        let path = exe.to_str().ok_or("the app path is not valid text")?;
        let inner = AutoLaunchBuilder::new()
            .set_app_name(app_name)
            .set_app_path(path)
            .set_macos_launch_mode(MacOSLaunchMode::LaunchAgent)
            // Per user: never asks for administrator rights.
            .set_windows_enable_mode(WindowsEnableMode::CurrentUser)
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self { inner })
    }

    /// The system is the source of truth; an unreadable state counts as "off".
    pub fn is_enabled(&self) -> bool {
        self.inner.is_enabled().unwrap_or(false)
    }

    pub fn set(&self, enabled: bool) -> Result<(), String> {
        let result = if enabled {
            self.inner.enable()
        } else {
            self.inner.disable()
        };
        result.map_err(|error| error.to_string())
    }

    /// If the app was set to start at login under its previous name, moves that entry
    /// to this one (the old entry would open the old app, or nothing). Best effort.
    pub fn take_over(&self, previous: &Autostart) {
        if previous.is_enabled() && previous.set(false).is_ok() {
            let _ = self.set(true);
        }
    }

    /// Rewrites the registration with the current path, so moving the app does not leave
    /// a login item pointing at the old location. Best effort and silent.
    pub fn refresh_if_enabled(&self) {
        if self.is_enabled() {
            let _ = self.set(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_installed_app_is_allowed() {
        for path in [
            "/Applications/CuantoQuedapp.app/Contents/MacOS/cuantoquedapp",
            "C:/Users/x/AppData/Local/cuantoquedapp/cuantoquedapp.exe",
            "/Users/x/Applications/CuantoQuedapp.app/Contents/MacOS/cuantoquedapp",
        ] {
            assert_eq!(eligibility(Path::new(path)), Eligibility::Allowed, "{path}");
        }
    }

    #[test]
    fn cargo_build_folders_are_development_builds() {
        for path in [
            "/Users/x/work/cuantoquedapp/target/debug/cuantoquedapp",
            "/Users/x/work/cuantoquedapp/target/release/cuantoquedapp",
            "C:/work/cuantoquedapp/target/x86_64-pc-windows-msvc/release/cuantoquedapp.exe",
        ] {
            assert_eq!(
                eligibility(Path::new(path)),
                Eligibility::DevelopmentBuild,
                "{path}"
            );
        }
    }

    #[test]
    fn a_folder_merely_named_target_is_not_a_cargo_build() {
        let path = Path::new("/Applications/target/CuantoQuedapp.app/Contents/MacOS/m");
        assert_eq!(eligibility(path), Eligibility::Allowed);
    }

    #[test]
    fn paths_that_would_break_the_plist_are_rejected() {
        for path in ["/Applications/A&B.app/x", "/Applications/<x>/y"] {
            assert_eq!(
                eligibility(Path::new(path)),
                Eligibility::UnsafePath,
                "{path}"
            );
        }
    }

    /// Touches the real `~/Library/LaunchAgents`, so it only runs on request:
    /// `cargo test real_launch_agent -- --ignored`. Uses a unique name and cleans up.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore]
    fn real_launch_agent_roundtrip() {
        use std::path::PathBuf;

        let name = format!("cuantoquedapp-test-{}", std::process::id());
        // The crate requires the app path to exist: use the test executable itself.
        let exe = std::env::current_exe().unwrap();
        let plist = PathBuf::from(std::env::var("HOME").unwrap())
            .join("Library/LaunchAgents")
            .join(format!("{name}.plist"));
        let autostart = Autostart::new(&name, &exe).unwrap();

        assert!(!autostart.is_enabled());
        autostart.set(true).unwrap();
        assert!(autostart.is_enabled());
        let contents = std::fs::read_to_string(&plist).unwrap();
        assert!(contents.contains("<key>RunAtLoad</key>"));
        assert!(contents.contains(exe.to_str().unwrap()));

        autostart.set(false).unwrap();
        assert!(!autostart.is_enabled());
        assert!(!plist.exists());
    }
}
