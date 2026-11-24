//! Live Codex source for the long-running icon: keeps one `codex app-server` process alive
//! between polls and turns its answer into `LimitsEvent`s.
//!
//! Live data is a bonus over the session files, so every failure is silent here: the process
//! is dropped, the poll returns nothing, and the next poll starts a fresh process. The files
//! keep the report complete and show how old their data is.
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::{
    app_server::{AppServer, DEFAULT_TIMEOUT},
    codex_binary::{Locations, find_codex_binary},
    live_codex::events_from_answer,
    reading::LimitsEvent,
};

pub struct LiveSource {
    binary: Option<PathBuf>,
    server: Option<AppServer>,
}

impl LiveSource {
    /// Looks for the native `codex` using the real environment.
    pub fn from_environment() -> Self {
        Self::with_binary(find_codex_binary(&Locations::from_environment()))
    }

    /// `None` makes every poll return nothing (Codex not installed or not found).
    pub fn with_binary(binary: Option<PathBuf>) -> Self {
        LiveSource {
            binary,
            server: None,
        }
    }

    /// Live events stamped with `now`, or an empty list if anything goes wrong.
    pub fn poll(&mut self, now: DateTime<Utc>) -> Vec<LimitsEvent> {
        let Some(binary) = &self.binary else {
            return Vec::new();
        };
        if self.server.is_none() {
            self.server = AppServer::start(binary, DEFAULT_TIMEOUT).ok();
        }
        let Some(server) = &mut self.server else {
            return Vec::new();
        };
        let events = server
            .read_rate_limits(DEFAULT_TIMEOUT)
            .ok()
            .and_then(|answer| events_from_answer(&answer, now).ok());
        if events.is_none() {
            // Dropping the client kills the process; the next poll starts a new one.
            self.server = None;
        }
        events.unwrap_or_default()
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-20T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    /// A fake `codex` that answers one limits request and then exits, so the second poll
    /// finds a dead process.
    fn one_shot_codex(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("cuantoquedapp-live-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let script = r#"#!/bin/sh
while IFS= read -r line; do
  id=${line#*\"id\":}; id=${id%%,*}
  case "$line" in
    *'"method":"initialize"'*) echo "{\"id\":$id,\"result\":{}}" ;;
    *'"method":"account/rateLimits/read"'*)
      echo "{\"id\":$id,\"result\":{\"rateLimits\":{\"limitId\":\"codex\",\"primary\":{\"usedPercent\":42,\"windowDurationMins\":300,\"resetsAt\":1795231411}}}}"
      exit 0 ;;
  esac
done
"#;
        let path = folder.join("codex");
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn without_a_binary_every_poll_is_empty() {
        let mut source = LiveSource::with_binary(None);
        assert!(source.poll(now()).is_empty());
    }

    #[test]
    fn a_binary_that_cannot_start_is_silent() {
        let mut source =
            LiveSource::with_binary(Some(PathBuf::from("/nonexistent/cuantoquedapp/codex")));
        assert!(source.poll(now()).is_empty());
        assert!(source.poll(now()).is_empty());
    }

    #[test]
    fn a_failed_poll_restarts_the_process_on_the_next_one() {
        let binary = one_shot_codex("restart");
        let mut source = LiveSource::with_binary(Some(binary.clone()));

        let first = source.poll(now());
        assert_eq!(first.len(), 1);
        assert_eq!(
            first[0].rate_limits.primary.as_ref().unwrap().used_percent,
            42.0
        );

        // The process exited after answering: this poll fails silently and drops it...
        assert!(source.poll(now()).is_empty());
        assert!(source.server.is_none());
        // ...and the following one starts a fresh process that works again.
        assert_eq!(source.poll(now()).len(), 1);

        drop(source);
        let _ = std::fs::remove_dir_all(binary.parent().unwrap());
    }
}
