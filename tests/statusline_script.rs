//! Runs `scripts/claude-statusline.sh` against a temporary HOME and settings file.
//! Unix only (bash + python3), like the script itself.
#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

struct Sandbox(PathBuf);

impl Sandbox {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "cuantoquedapp-script-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(path.join(".claude")).unwrap();
        Sandbox(path)
    }

    fn settings(&self) -> PathBuf {
        self.0.join(".claude/settings.json")
    }

    fn write_settings(&self, text: &str) {
        fs::write(self.settings(), text).unwrap();
    }

    /// An executable "helper" at `relative`, printing `helper-ran`.
    fn helper(&self, relative: &str) -> PathBuf {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "#!/bin/sh\necho helper-ran\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn run(&self, action: &str, helper: &Path) -> (bool, String) {
        let output = Command::new("bash")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/claude-statusline.sh"
            ))
            .arg(action)
            .env("HOME", &self.0)
            .env("CLAUDE_SETTINGS", self.settings())
            .env("CUANTOQUEDAPP_STATUSLINE", helper)
            .output()
            .unwrap();
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).into_owned()
                + &String::from_utf8_lossy(&output.stderr),
        )
    }

    fn status_line_command(&self) -> String {
        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(self.settings()).unwrap()).unwrap();
        value["statusLine"]["command"].as_str().unwrap().to_string()
    }

    fn run_command(&self, command: &str) -> String {
        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&self.0)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const ORIGINAL: &str = r#"{"model":"x","statusLine":{"type":"command","command":"echo original"}}"#;

// A2
#[test]
fn a_hostile_helper_path_stays_one_literal_word() {
    let sandbox = Sandbox::new("hostile");
    sandbox.write_settings(ORIGINAL);
    // Spaces, quotes, `$()` and a backtick: none of it may be interpreted by the shell.
    let helper = sandbox.helper("we ird/$(touch PWNED) `touch PWNED2` 'q' \"d\"/helper");
    let (ok, message) = sandbox.run("install", &helper);
    assert!(ok, "{message}");
    let command = sandbox.status_line_command();
    let printed = sandbox.run_command(&command);
    assert_eq!(
        printed.trim(),
        "helper-ran",
        "the helper ran, once, as a single word"
    );
    assert!(!sandbox.0.join("PWNED").exists() && !sandbox.0.join("PWNED2").exists());
}

#[test]
fn install_status_and_uninstall_restore_the_original_and_never_overwrite_backups() {
    let sandbox = Sandbox::new("roundtrip");
    sandbox.write_settings(ORIGINAL);
    let helper = sandbox.helper("bin/helper");
    assert!(sandbox.run("install", &helper).0);
    assert!(sandbox.run("status", &helper).1.contains("Installed"));
    let (ok, message) = sandbox.run("install", &helper);
    assert!(ok && message.contains("Already installed"));
    // The original command is still what runs when the helper disappears.
    fs::remove_file(&helper).unwrap();
    assert_eq!(
        sandbox.run_command(&sandbox.status_line_command()).trim(),
        "original"
    );
    let helper = sandbox.helper("bin/helper");
    assert!(sandbox.run("refresh", &helper).0);
    assert!(sandbox.run("uninstall", &helper).0);
    let restored: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(sandbox.settings()).unwrap()).unwrap();
    let original: serde_json::Value = serde_json::from_str(ORIGINAL).unwrap();
    assert_eq!(restored, original);
    let backups = fs::read_dir(sandbox.0.join(".claude"))
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".bak")
        })
        .count();
    assert!(
        backups >= 2,
        "each change keeps its own backup, found {backups}"
    );
    // No temporary files left behind.
    assert!(
        !fs::read_dir(sandbox.0.join(".claude"))
            .unwrap()
            .any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp"))
    );
}

#[test]
fn the_permissions_of_settings_are_kept() {
    let sandbox = Sandbox::new("perms");
    sandbox.write_settings(ORIGINAL);
    fs::set_permissions(sandbox.settings(), fs::Permissions::from_mode(0o600)).unwrap();
    let helper = sandbox.helper("bin/helper");
    assert!(sandbox.run("install", &helper).0);
    let mode = fs::metadata(sandbox.settings())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn unreadable_settings_are_left_untouched() {
    let sandbox = Sandbox::new("broken");
    sandbox.write_settings("{ not json");
    let helper = sandbox.helper("bin/helper");
    let (ok, _) = sandbox.run("install", &helper);
    assert!(!ok);
    assert_eq!(
        fs::read_to_string(sandbox.settings()).unwrap(),
        "{ not json"
    );
}
