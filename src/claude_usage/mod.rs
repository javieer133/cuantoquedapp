//! Claude limits asked to the official client: `claude -p "/usage"` prints the same numbers as
//! the in-app `/usage` panel. It is a local command (no model call, no cost, no session file
//! with `--no-session-persistence`), and the client uses its own login: this app never sees a token.
//!
//! The output is human text, not a documented format: the parser is strict and returns `None`
//! instead of guessing, so a changed format means "no data", never a wrong number.
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use chrono::{DateTime, TimeZone, Utc};

use crate::{debug_log::debug_log, reading::LimitsEvent};

mod locate;
mod parse;
mod run;
#[cfg(test)]
mod tests;

pub use locate::{Locations, find_claude_binary};
pub use parse::parse_usage;
pub use run::run_usage;

/// Environment variable that points straight to the `claude` executable.
pub const ENV_OVERRIDE: &str = "CUANTOQUEDAPP_CLAUDE";
/// How long to wait for `claude` before giving up (it normally answers in ~2 s).
pub const TIMEOUT: Duration = Duration::from_secs(20);
/// Minimum time between two real calls: the limits change slowly and each call starts a process.
pub const MIN_INTERVAL: Duration = Duration::from_secs(300);

/// Polls `/usage` at most once per `MIN_INTERVAL` and remembers the last good answer.
pub struct ClaudeUsageSource {
    binary: Option<PathBuf>,
    last_try: Option<Instant>,
    last: Option<LimitsEvent>,
}

impl ClaudeUsageSource {
    pub fn from_environment() -> Self {
        Self::with_binary(find_claude_binary(&Locations::from_environment()))
    }

    pub fn with_binary(binary: Option<PathBuf>) -> Self {
        Self {
            binary,
            last_try: None,
            last: None,
        }
    }

    /// The next `poll` asks `claude` again even if the interval has not passed (manual refresh).
    pub fn force_next(&mut self) {
        self.last_try = None;
    }

    /// The freshest answer: a new one if the interval has passed, else the remembered one
    /// (its timestamp stays the real reading time, so the age shown is honest).
    /// Empty if there is no binary or it never answered.
    pub fn poll<T: TimeZone>(&mut self, now: DateTime<Utc>, zone: &T) -> Vec<LimitsEvent> {
        let due = self.last_try.is_none_or(|at| at.elapsed() >= MIN_INTERVAL);
        if due {
            self.last_try = Some(Instant::now());
            match &self.binary {
                None => debug_log("claude: no executable found (set CUANTOQUEDAPP_CLAUDE)"),
                Some(binary) => {
                    let name = binary
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    debug_log(&format!("claude: running {name} -p /usage"));
                    match run_usage(binary, TIMEOUT) {
                        None => debug_log("claude: failed, exited with an error or timed out"),
                        Some(text) => match parse_usage(&text, now, zone) {
                            Some(event) => {
                                debug_log("claude: usage parsed");
                                self.last = Some(event);
                            }
                            None => {
                                // Only the size and the kind of failure: never text from the output.
                                let kind = if text.contains("Total cost") {
                                    "this login shows costs, not plan limits (API key or console account); sign in with a subscription"
                                } else {
                                    "format not understood"
                                };
                                debug_log(&format!(
                                    "claude: output not understood ({} bytes): {kind}",
                                    text.len()
                                ));
                            }
                        },
                    }
                }
            }
        }
        self.last.iter().cloned().collect()
    }
}
