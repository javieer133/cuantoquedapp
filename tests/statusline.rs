//! Unix only: the paths and the previous-command chain follow the macOS layout.
//! Runs the real `cuantoquedapp-statusline` binary against a temporary HOME.
#![cfg(unix)]

use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

struct TempHome(PathBuf);

impl TempHome {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "cuantoquedapp-statusline-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        TempHome(path)
    }

    fn limits_file(&self) -> PathBuf {
        self.0
            .join("Library/Caches/cuantoquedapp/claude-limits.json")
    }

    fn set_chain(&self, command: &str) {
        let folder = self.0.join(".claude/cuantoquedapp");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("chain.sh"), command).unwrap();
    }

    /// Runs the helper with `input` on stdin. Returns (stdout, exit code).
    fn run(&self, input: &str) -> (String, i32) {
        let mut child = Command::new(env!("CARGO_BIN_EXE_cuantoquedapp-statusline"))
            .env("HOME", &self.0)
            .env_remove("LOCALAPPDATA")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        (
            String::from_utf8_lossy(&output.stdout).into_owned(),
            output.status.code().unwrap(),
        )
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const INPUT: &str = r#"{"session_id":"s3cret","rate_limits":{"five_hour":{"used_percentage":12.5,"resets_at":1738425600}}}"#;

#[test]
fn saves_only_the_limits_and_prints_nothing_without_a_chain() {
    let home = TempHome::new("plain");
    let (stdout, code) = home.run(INPUT);
    assert_eq!((stdout.as_str(), code), ("", 0));
    let saved = std::fs::read_to_string(home.limits_file()).unwrap();
    assert!(saved.contains("12.5") && !saved.contains("s3cret"));
}

#[cfg(unix)]
#[test]
fn passes_the_same_input_to_the_previous_command_and_keeps_its_exit_code() {
    let home = TempHome::new("chain");
    home.set_chain("cat; echo ' <-chained'; exit 3");
    let (stdout, code) = home.run(INPUT);
    assert_eq!(code, 3);
    assert!(stdout.starts_with(INPUT) && stdout.ends_with("<-chained\n"));
    assert!(home.limits_file().exists());
}

// SEC-02: the chain is run as a shell command, so only a plain, private file counts.
#[test]
fn a_linked_or_shared_chain_is_not_run() {
    use std::os::unix::fs::PermissionsExt;
    let home = TempHome::new("untrusted");
    home.set_chain("echo ran");
    let chain = home.0.join(".claude/cuantoquedapp/chain.sh");
    std::fs::set_permissions(&chain, std::fs::Permissions::from_mode(0o666)).unwrap();
    assert_eq!(home.run(INPUT), (String::new(), 0), "world-writable");

    let elsewhere = home.0.join("elsewhere.sh");
    std::fs::write(&elsewhere, "echo ran").unwrap();
    std::fs::remove_file(&chain).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &chain).unwrap();
    assert_eq!(home.run(INPUT), (String::new(), 0), "a link");

    std::fs::remove_file(&chain).unwrap();
    home.set_chain("echo ran");
    assert_eq!(
        home.run(INPUT),
        ("ran\n".to_string(), 0),
        "a plain private file runs"
    );
}

#[test]
fn input_without_limits_keeps_the_previous_capture() {
    let home = TempHome::new("keep");
    home.run(INPUT);
    let before = std::fs::read_to_string(home.limits_file()).unwrap();
    let (_, code) = home.run(r#"{"model":"x"}"#);
    assert_eq!(code, 0);
    assert_eq!(std::fs::read_to_string(home.limits_file()).unwrap(), before);
}

#[test]
fn garbage_input_never_fails() {
    let home = TempHome::new("garbage");
    assert_eq!(home.run("\u{0}\u{1}not json").1, 0);
    assert!(!home.limits_file().exists());
}
