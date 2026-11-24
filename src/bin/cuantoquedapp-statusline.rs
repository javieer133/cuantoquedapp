//! Claude Code `statusLine` helper: keeps the rate limits from the input and then hands the
//! same input to the command that was configured before (so nothing else changes).
//!
//! Safe by design: it never prints anything of its own, never fails because of the capture,
//! and keeps only the limits fields (see `claude_limits`). The previous command, if any, is
//! read from `~/.claude/cuantoquedapp/chain.sh`; installation is `scripts/claude-statusline.sh`.
use std::io::Read;

use chrono::Utc;
use cuantoquedapp::claude_limits::{MAX_INPUT_BYTES, capture, default_limits_path, save};

fn main() {
    let mut input = Vec::new();
    // A read error just means less input; the status line must never break because of us.
    let _ = std::io::stdin()
        .lock()
        .take(MAX_INPUT_BYTES)
        .read_to_end(&mut input);

    if let (Some(path), Some(stored)) = (default_limits_path(), capture(&input, Utc::now())) {
        let _ = save(&path, &stored);
    }

    #[cfg(unix)]
    if let Some(code) = run_previous_command(&input) {
        std::process::exit(code);
    }
}

#[cfg(unix)]
fn chain_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").filter(|home| !home.is_empty())?;
    Some(std::path::PathBuf::from(home).join(".claude/cuantoquedapp/chain.sh"))
}

/// The saved command, read only from a plain file that nobody else can change: it is run as
/// a shell command, so a link or a group/world-writable file is ignored.
#[cfg(unix)]
fn trusted_chain(path: &std::path::Path) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.file_type().is_file() || meta.permissions().mode() & 0o022 != 0 {
        return None;
    }
    let bytes = cuantoquedapp::small_file::read(path, cuantoquedapp::small_file::MAX_BYTES).ok()?;
    String::from_utf8(bytes).ok()
}

/// Runs the saved previous command with the original input; its output and exit code are ours.
/// `None` when there is nothing to chain.
#[cfg(unix)]
fn run_previous_command(input: &[u8]) -> Option<i32> {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };

    let command = trusted_chain(&chain_path()?)?;
    if command.trim().is_empty() {
        return None;
    }
    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::piped())
        .spawn()
        .ok()?;
    if let Some(mut stdin) = child.stdin.take() {
        // The previous command may not read its input at all (a broken pipe is fine).
        let _ = stdin.write_all(input);
    }
    Some(child.wait().ok()?.code().unwrap_or(1))
}
