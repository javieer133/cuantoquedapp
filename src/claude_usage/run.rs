//! Runs `claude -p "/usage"` as a child process without ever blocking the caller for longer
//! than the given time, whatever `claude` or its children do with the pipe.
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use crate::process_tree::ProcessTree;

/// Variables that make Claude Code bill an API key (or a gateway) instead of using the
/// subscription login. With them `/usage` prints costs, not the plan limits we want.
pub(super) const BILLING_OVERRIDES: [&str; 2] = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"];

pub(super) fn usage_command(binary: &Path) -> Command {
    let mut command = Command::new(binary);
    command
        .args([
            "-p",
            "/usage",
            "--no-session-persistence",
            "--output-format",
            "text",
        ])
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // Only for this child process: the user's own environment is not touched.
    for name in BILLING_OVERRIDES {
        command.env_remove(name);
    }
    command
}

/// After `claude` exits, how long to wait for its output to reach end-of-file. A leftover
/// child process can keep the pipe open forever; what was read by then is still used.
const OUTPUT_GRACE: Duration = Duration::from_millis(500);
/// Most output ever kept (the real one is a few hundred bytes).
const MAX_OUTPUT: u64 = 1 << 20;

/// Runs `claude -p "/usage"` and returns its text, or `None` on any failure or timeout.
/// Never blocks longer than `timeout` plus `OUTPUT_GRACE`, whatever `claude` or its
/// children do with the pipe.
pub fn run_usage(binary: &Path, timeout: Duration) -> Option<String> {
    let mut command = usage_command(binary);
    // Dropping it (any return below) kills `claude` and whatever it started or left running.
    let mut process = ProcessTree::spawn(&mut command).ok()?;
    let mut stdout = process.child.stdout.take()?;
    // The reader fills a shared buffer, so a partial read is still available if the pipe
    // never closes; the channel only says "end of file reached".
    let collected = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    let (done_sender, done) = std::sync::mpsc::channel::<()>();
    let sink = std::sync::Arc::clone(&collected);
    std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        loop {
            match stdout.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    let Ok(mut buffer) = sink.lock() else { break };
                    if buffer.len() as u64 >= MAX_OUTPUT {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..read]);
                }
            }
        }
        let _ = done_sender.send(());
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match process.child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            _ => return None,
        }
    };
    let _ = done.recv_timeout(OUTPUT_GRACE);
    let bytes = collected.lock().ok()?.clone();
    status
        .success()
        .then(|| String::from_utf8_lossy(&bytes).into_owned())
}
