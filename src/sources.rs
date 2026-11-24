//! Every live data source in one place, used by the tray icon and by `cuantoquedapp-cli --live`:
//! Codex through `codex app-server`, Claude through `claude -p "/usage"`, and the status line
//! capture as a fallback for Claude.
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

use chrono::{DateTime, TimeZone, Utc};

use crate::{
    claude_limits, claude_usage::ClaudeUsageSource, live_source::LiveSource, reading::LimitsEvent,
};

/// Which tools to ask live. A hidden tool is not asked (asking Claude costs a few seconds of
/// CPU each time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tools {
    pub codex: bool,
    pub claude: bool,
}

impl Tools {
    pub const ALL: Tools = Tools {
        codex: true,
        claude: true,
    };
}

/// `Tools` shared between the interface (which changes it) and the reading thread.
#[derive(Debug)]
pub struct SharedTools {
    codex: AtomicBool,
    claude: AtomicBool,
}

impl SharedTools {
    pub fn new(tools: Tools) -> Self {
        SharedTools {
            codex: AtomicBool::new(tools.codex),
            claude: AtomicBool::new(tools.claude),
        }
    }

    pub fn get(&self) -> Tools {
        Tools {
            codex: self.codex.load(Ordering::Relaxed),
            claude: self.claude.load(Ordering::Relaxed),
        }
    }

    /// Stores `tools` and returns the previous value.
    pub fn set(&self, tools: Tools) -> Tools {
        Tools {
            codex: self.codex.swap(tools.codex, Ordering::Relaxed),
            claude: self.claude.swap(tools.claude, Ordering::Relaxed),
        }
    }
}

pub struct Sources {
    codex: Option<LiveSource>,
    claude: Option<ClaudeUsageSource>,
    claude_file: Option<PathBuf>,
}

impl Sources {
    pub fn new(
        codex: Option<LiveSource>,
        claude: Option<ClaudeUsageSource>,
        claude_file: Option<PathBuf>,
    ) -> Self {
        Sources {
            codex,
            claude,
            claude_file,
        }
    }

    /// Looks for the executables and the capture file in the real environment.
    pub fn from_environment() -> Self {
        Sources::new(
            Some(LiveSource::from_environment()),
            Some(ClaudeUsageSource::from_environment()),
            claude_limits::default_limits_path(),
        )
    }

    /// Live events stamped with `now`. `claude /usage` is the exact source; the status line
    /// capture is only used when it gives nothing (client missing, failing or logged out),
    /// because it can lag behind. With `force`, Claude is asked again even if it was asked
    /// moments ago. Only the tools in `tools` are asked.
    pub fn poll<T: TimeZone>(
        &mut self,
        now: DateTime<Utc>,
        zone: &T,
        force: bool,
        tools: Tools,
    ) -> Vec<LimitsEvent> {
        let mut events = match &mut self.codex {
            Some(source) if tools.codex => source.poll(now),
            _ => Vec::new(),
        };
        if !tools.claude {
            return events;
        }
        let mut claude = self
            .claude
            .as_mut()
            .map(|source| {
                if force {
                    source.force_next();
                }
                source.poll(now, zone)
            })
            .unwrap_or_default();
        if claude.is_empty()
            && let Some(path) = &self.claude_file
        {
            claude = claude_limits::read_events(path);
        }
        events.extend(claude);
        events
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::claude_limits::{Stored, StoredWindow, save};

    fn stored_file(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "cuantoquedapp-sources-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        let path = folder.join("claude-limits.json");
        let window = StoredWindow {
            used_percentage: 13.0,
            resets_at: Utc::now().timestamp() + 3600,
        };
        save(
            &path,
            &Stored {
                captured_at: "2026-11-21T00:00:00.000Z".into(),
                five_hour: Some(window),
                seven_day: None,
            },
        )
        .unwrap();
        path
    }

    #[test]
    fn no_sources_give_no_events() {
        let mut sources = Sources::new(None, None, None);
        assert!(sources.poll(Utc::now(), &Utc, false, Tools::ALL).is_empty());
    }

    #[test]
    fn the_capture_is_the_fallback_when_claude_gives_nothing() {
        let path = stored_file("fallback");
        let mut sources = Sources::new(
            None,
            Some(ClaudeUsageSource::with_binary(None)),
            Some(path.clone()),
        );
        let events = sources.poll(Utc::now(), &Utc, false, Tools::ALL);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].rate_limits.limit_id, "claude");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn claude_usage_wins_over_the_capture_and_force_asks_again() {
        use std::os::unix::fs::PermissionsExt;
        let path = stored_file("wins");
        let dir = path.parent().unwrap().to_path_buf();
        let counter = dir.join("calls");
        let binary = dir.join("claude");
        std::fs::write(
            &binary,
            format!(
                "#!/bin/sh\necho x >> '{}'\necho 'Current session: 43% used · resets 11pm (X)'\n",
                counter.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut sources = Sources::new(
            None,
            Some(ClaudeUsageSource::with_binary(Some(binary))),
            Some(path),
        );
        let now = Utc::now();
        let events = sources.poll(now, &Utc, false, Tools::ALL);
        assert_eq!(
            events[0].rate_limits.primary.as_ref().unwrap().used_percent,
            43.0
        );
        // Within the interval a normal poll reuses the answer; a forced one asks again.
        sources.poll(now, &Utc, false, Tools::ALL);
        assert_eq!(
            std::fs::read_to_string(&counter).unwrap().lines().count(),
            1
        );
        sources.poll(now, &Utc, true, Tools::ALL);
        assert_eq!(
            std::fs::read_to_string(&counter).unwrap().lines().count(),
            2
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_hidden_tool_is_not_asked_and_the_shared_value_swaps() {
        let path = stored_file("hidden");
        let mut sources = Sources::new(
            None,
            Some(ClaudeUsageSource::with_binary(None)),
            Some(path.clone()),
        );
        let no_claude = Tools {
            codex: true,
            claude: false,
        };
        assert!(sources.poll(Utc::now(), &Utc, false, no_claude).is_empty());
        assert_eq!(sources.poll(Utc::now(), &Utc, false, Tools::ALL).len(), 1);
        let shared = SharedTools::new(Tools::ALL);
        assert_eq!(shared.set(no_claude), Tools::ALL);
        assert_eq!(shared.get(), no_claude);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
