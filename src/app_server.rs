//! Minimal client for `codex app-server`: JSON-RPC over stdio, one JSON object per line
//! (this protocol has no `"jsonrpc"` field).
//!
//! It starts the process, introduces itself (`initialize` + `initialized`) and answers
//! requests with a timeout. Notifications and server-initiated requests are ignored.
//! The child and its own children are killed when the client is dropped, and it also exits on its own when this
//! process dies (its stdin closes).
use std::{
    fmt,
    io::{BufReader, Write},
    path::Path,
    process::{ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use crate::{
    bounded_line::{LineRead, read_line_bounded},
    process_tree::ProcessTree,
};

/// How long to wait for the handshake and for each request.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Longest line kept from the server: a rate-limits reply is a few KiB. Longer lines (none
/// that we need) are dropped without being held whole.
pub const MAX_REPLY_BYTES: usize = 1024 * 1024;
/// Lines waiting to be read, at most.
pub const QUEUE_LINES: usize = 32;

#[derive(Debug)]
pub enum AppServerError {
    /// The process could not be started.
    Spawn(std::io::Error),
    /// Writing to the process failed.
    Io(std::io::ErrorKind),
    /// No answer before the deadline.
    Timeout,
    /// The process ended.
    Closed,
    /// The server answered with an error. Only its code is kept: the text could hold account data.
    Server(Option<i64>),
}

impl fmt::Display for AppServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(error) => write!(f, "could not start Codex ({:?})", error.kind()),
            Self::Io(kind) => write!(f, "could not talk to Codex ({kind:?})"),
            Self::Timeout => write!(f, "Codex did not answer in time"),
            Self::Closed => write!(f, "Codex closed"),
            Self::Server(Some(code)) => write!(f, "Codex answered with an error (code {code})"),
            Self::Server(None) => write!(f, "Codex answered with an error"),
        }
    }
}

impl std::error::Error for AppServerError {}

pub struct AppServer {
    /// Dropping it kills the server and everything it started.
    _process: ProcessTree,
    stdin: ChildStdin,
    lines: Receiver<String>,
    next_id: u64,
}

impl AppServer {
    /// Starts `<binary> app-server` and completes the handshake.
    pub fn start(binary: &Path, timeout: Duration) -> Result<Self, AppServerError> {
        let mut command = Command::new(binary);
        command
            .arg("app-server")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut process = ProcessTree::spawn(&mut command).map_err(AppServerError::Spawn)?;

        let (Some(stdin), Some(stdout)) = (process.child.stdin.take(), process.child.stdout.take())
        else {
            return Err(AppServerError::Closed);
        };
        // Bounded: a server that talks without being asked cannot grow our memory (the
        // reader waits when the queue is full) and an enormous line is dropped. At most
        // QUEUE_LINES * MAX_REPLY_BYTES (32 MiB) is ever held.
        let (sender, lines) = mpsc::sync_channel(QUEUE_LINES);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut bytes = Vec::new();
            loop {
                match read_line_bounded(&mut reader, &mut bytes, MAX_REPLY_BYTES) {
                    Ok(LineRead::Complete) => {
                        let line = String::from_utf8_lossy(&bytes).trim_end().to_string();
                        if sender.send(line).is_err() {
                            break;
                        }
                    }
                    Ok(LineRead::TooLong { .. }) => {}
                    Ok(LineRead::Eof) | Err(_) => break,
                }
            }
        });

        // From here on dropping `server` kills the process if anything below fails.
        let mut server = Self {
            _process: process,
            stdin,
            lines,
            next_id: 1,
        };
        let client_info = json!({
            "clientInfo": {"name": "cuantoquedapp", "version": env!("CARGO_PKG_VERSION")}
        });
        server.request("initialize", Some(client_info), timeout)?;
        server.send(&json!({"method": "initialized"}))?;
        Ok(server)
    }

    /// The current limits, exactly as the server answers (`account/rateLimits/read`).
    pub fn read_rate_limits(&mut self, timeout: Duration) -> Result<Value, AppServerError> {
        self.request("account/rateLimits/read", None, timeout)
    }

    fn request(
        &mut self,
        method: &str,
        params: Option<Value>,
        timeout: Duration,
    ) -> Result<Value, AppServerError> {
        let id = self.next_id;
        self.next_id += 1;
        let mut message = json!({"id": id, "method": method});
        if let Some(params) = params {
            message["params"] = params;
        }
        self.send(&message)?;

        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let line = match self.lines.recv_timeout(remaining) {
                Ok(line) => line,
                Err(RecvTimeoutError::Timeout) => return Err(AppServerError::Timeout),
                Err(RecvTimeoutError::Disconnected) => return Err(AppServerError::Closed),
            };
            let Ok(answer) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            // A response has our id and no `method`; a server request may reuse the same number.
            if answer.get("id") != Some(&json!(id)) || answer.get("method").is_some() {
                continue;
            }
            if let Some(error) = answer.get("error") {
                return Err(AppServerError::Server(
                    error.get("code").and_then(Value::as_i64),
                ));
            }
            return Ok(answer.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    fn send(&mut self, message: &Value) -> Result<(), AppServerError> {
        let mut line = message.to_string();
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|()| self.stdin.flush())
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::BrokenPipe => AppServerError::Closed,
                kind => AppServerError::Io(kind),
            })
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::{os::unix::fs::PermissionsExt, path::PathBuf};

    use super::*;

    /// A fake `codex` in a fresh temp folder. `rate_limits_reply` is the shell code that
    /// answers `account/rateLimits/read` (it can use `$id`, the request id).
    fn fake_codex(name: &str, rate_limits_reply: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("cuantoquedapp-fake-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let script = format!(
            r#"#!/bin/sh
echo $$ > "{pidfile}"
echo '{{"method":"remoteControl/status/changed","params":{{}}}}'
while IFS= read -r line; do
  id=${{line#*\"id\":}}; id=${{id%%,*}}
  case "$line" in
    *'"method":"initialize"'*) echo "{{\"id\":$id,\"result\":{{\"userAgent\":\"fake\"}}}}" ;;
    *'"method":"account/rateLimits/read"'*) {reply} ;;
  esac
done
"#,
            pidfile = folder.join("pid").display(),
            reply = rate_limits_reply,
        );
        let path = folder.join("codex");
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    const OK_REPLY: &str = r#"echo '{"method":"account/updated","params":{}}'
      echo "{\"id\":$id,\"method\":\"item/approval\",\"params\":{}}"
      echo "{\"id\":$id,\"result\":{\"rateLimits\":{\"limitId\":\"codex\",\"primary\":{\"usedPercent\":42,\"windowDurationMins\":300,\"resetsAt\":1795231411}}}}""#;

    fn cleanup(binary: &Path) {
        let _ = std::fs::remove_dir_all(binary.parent().unwrap());
    }

    #[test]
    fn reads_the_limits_ignoring_noise_and_server_requests() {
        let binary = fake_codex("ok", OK_REPLY);
        let mut server = AppServer::start(&binary, Duration::from_secs(5)).unwrap();
        let answer = server.read_rate_limits(Duration::from_secs(5)).unwrap();
        assert_eq!(answer["rateLimits"]["primary"]["usedPercent"], 42);
        // A second request on the same process works too (ids keep increasing).
        let again = server.read_rate_limits(Duration::from_secs(5)).unwrap();
        assert_eq!(again["rateLimits"]["limitId"], "codex");
        drop(server);
        cleanup(&binary);
    }

    #[test]
    fn a_server_error_keeps_only_its_code() {
        let binary = fake_codex(
            "error",
            r#"echo "{\"id\":$id,\"error\":{\"code\":-32000,\"message\":\"secret account text\"}}""#,
        );
        let mut server = AppServer::start(&binary, Duration::from_secs(5)).unwrap();
        let error = server.read_rate_limits(Duration::from_secs(5)).unwrap_err();
        assert!(matches!(error, AppServerError::Server(Some(-32000))));
        let shown = format!("{error} / {error:?}");
        assert!(shown.contains("-32000"));
        assert!(!shown.contains("secret"));
        drop(server);
        cleanup(&binary);
    }

    #[test]
    fn silence_ends_in_a_timeout_not_a_hang() {
        let binary = fake_codex("silent", ":");
        let mut server = AppServer::start(&binary, Duration::from_secs(5)).unwrap();
        let started = Instant::now();
        let error = server
            .read_rate_limits(Duration::from_millis(300))
            .unwrap_err();
        assert!(matches!(error, AppServerError::Timeout));
        assert!(started.elapsed() < Duration::from_secs(3));
        drop(server);
        cleanup(&binary);
    }

    #[test]
    fn a_process_that_exits_is_reported_as_closed() {
        let binary = fake_codex("exits", "exit 0");
        let mut server = AppServer::start(&binary, Duration::from_secs(5)).unwrap();
        let error = server.read_rate_limits(Duration::from_secs(5)).unwrap_err();
        assert!(matches!(error, AppServerError::Closed), "{error:?}");
        drop(server);
        cleanup(&binary);
    }

    #[test]
    fn a_missing_binary_is_a_spawn_error() {
        let error = AppServer::start(Path::new("/nonexistent/codex"), Duration::from_secs(1))
            .err()
            .unwrap();
        assert!(matches!(error, AppServerError::Spawn(_)));
        assert!(error.to_string().starts_with("could not start Codex"));
    }

    #[test]
    fn a_failed_handshake_does_not_leave_the_process_running() {
        // Answers nothing at all: the handshake times out.
        let folder = std::env::temp_dir().join(format!(
            "cuantoquedapp-fake-nohandshake-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("codex");
        let pidfile = folder.join("pid");
        std::fs::write(
            &path,
            format!("#!/bin/sh\necho $$ > \"{}\"\nsleep 30\n", pidfile.display()),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();

        let error = AppServer::start(&path, Duration::from_millis(400))
            .err()
            .unwrap();
        assert!(matches!(error, AppServerError::Timeout));
        assert!(process_is_gone(&pidfile), "the fake codex is still running");
        cleanup(&path);
    }

    #[test]
    fn dropping_the_client_kills_the_process() {
        let binary = fake_codex("drop", OK_REPLY);
        let pidfile = binary.parent().unwrap().join("pid");
        let server = AppServer::start(&binary, Duration::from_secs(5)).unwrap();
        // The handshake already finished, so the script has written its PID.
        assert!(pidfile.exists());
        assert!(
            !process_is_gone_now(&pidfile),
            "the fake codex should be running"
        );
        drop(server);
        assert!(process_is_gone(&pidfile), "the fake codex is still running");
        cleanup(&binary);
    }

    #[test]
    fn dropping_the_client_also_kills_the_children_of_the_server() {
        let folder = std::env::temp_dir().join(format!(
            "cuantoquedapp-fake-grandchild-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let grandchild = folder.join("grandchild-pid");
        let path = folder.join("codex");
        std::fs::write(
            &path,
            format!(
                r#"#!/bin/sh
sleep 30 &
echo $! > "{}"
while IFS= read -r line; do
  id=${{line#*\"id\":}}; id=${{id%%,*}}
  echo "{{\"id\":$id,\"result\":{{}}}}"
done
"#,
                grandchild.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();

        let server = AppServer::start(&path, Duration::from_secs(5)).unwrap();
        assert!(!process_is_gone_now(&grandchild), "the child should run");
        drop(server);
        assert!(
            process_is_gone(&grandchild),
            "the child of the server is still running"
        );
        cleanup(&path);
    }

    fn pid_is_alive(pidfile: &Path) -> bool {
        let pid = std::fs::read_to_string(pidfile)
            .map(|text| text.trim().to_string())
            .unwrap_or_default();
        !pid.is_empty()
            && Command::new("kill")
                .args(["-0", &pid])
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
    }

    /// Checks right now, without waiting.
    fn process_is_gone_now(pidfile: &Path) -> bool {
        !pid_is_alive(pidfile)
    }

    /// True once the PID in `pidfile` no longer exists (waits up to 3 s).
    fn process_is_gone(pidfile: &Path) -> bool {
        for _ in 0..30 {
            if !pid_is_alive(pidfile) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    }
}
