//! Claude limits, captured from the Claude Code status line.
//!
//! Claude Code passes a JSON document to the `statusLine` command that includes
//! `rate_limits.five_hour` / `seven_day` (used percentage and reset time), but only for
//! subscribers and only after the first API response of a session. The helper binary
//! `cuantoquedapp-statusline` keeps just those fields in a small file; the app reads it here.
//!
//! Pure functions plus tiny file helpers; the clock is always a parameter. Nothing else from the
//! status line input (paths, prompts, model, costs…) is ever kept.
use std::{
    io,
    path::{Path, PathBuf},
};

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    codex::{RateLimits, Window},
    reading::LimitsEvent,
};

/// Limit id used for Claude in reports.
pub const LIMIT_ID: &str = "claude";
const FIVE_HOUR_MINUTES: u64 = 300;
const SEVEN_DAY_MINUTES: u64 = 10080;
/// Status line input is small; refuse anything absurd instead of reading it all.
pub const MAX_INPUT_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct Input {
    rate_limits: Option<InputLimits>,
}

#[derive(Debug, Deserialize)]
struct InputLimits {
    five_hour: Option<InputWindow>,
    seven_day: Option<InputWindow>,
}

#[derive(Debug, Deserialize)]
struct InputWindow {
    used_percentage: Option<f64>,
    resets_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StoredWindow {
    pub used_percentage: f64,
    pub resets_at: i64,
}

/// What the helper writes and the app reads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stored {
    pub captured_at: String,
    pub five_hour: Option<StoredWindow>,
    pub seven_day: Option<StoredWindow>,
}

fn stored_window(window: Option<InputWindow>) -> Option<StoredWindow> {
    let window = window?;
    let used = window.used_percentage.filter(|used| used.is_finite())?;
    Some(StoredWindow {
        used_percentage: used,
        resets_at: window.resets_at?,
    })
}

/// Limits found in one status line input. `None` when the input has no usable limits
/// (not valid JSON, no `rate_limits`, or no complete window): the caller then keeps what
/// it already had instead of overwriting it with nothing.
pub fn capture(input: &[u8], now: DateTime<Utc>) -> Option<Stored> {
    let input: Input = serde_json::from_slice(input).ok()?;
    let limits = input.rate_limits?;
    let five_hour = stored_window(limits.five_hour);
    let seven_day = stored_window(limits.seven_day);
    if five_hour.is_none() && seven_day.is_none() {
        return None;
    }
    Some(Stored {
        captured_at: now.to_rfc3339_opts(SecondsFormat::Millis, true),
        five_hour,
        seven_day,
    })
}

fn window(stored: Option<StoredWindow>, minutes: u64) -> Option<Window> {
    stored.map(|stored| Window {
        used_percent: stored.used_percentage,
        window_minutes: minutes,
        resets_at: stored.resets_at,
    })
}

/// The stored capture as the same event type the Codex sources produce.
pub fn event_from_stored(stored: &Stored) -> LimitsEvent {
    LimitsEvent {
        timestamp: stored.captured_at.clone(),
        rate_limits: RateLimits {
            limit_id: LIMIT_ID.to_string(),
            plan_type: None,
            primary: window(stored.five_hour, FIVE_HOUR_MINUTES),
            secondary: window(stored.seven_day, SEVEN_DAY_MINUTES),
            credits: None,
        },
    }
}

/// `~/Library/Caches/cuantoquedapp/claude-limits.json` (Windows: under `%LOCALAPPDATA%`),
/// next to the single-instance lock.
pub fn default_limits_path() -> Option<PathBuf> {
    let lock = crate::single_instance::default_lock_path()?;
    Some(lock.parent()?.join("claude-limits.json"))
}

/// Writes the capture atomically so a reader never sees half a file and two sessions that
/// save at the same time never share a temporary file.
pub fn save(path: &Path, stored: &Stored) -> io::Result<()> {
    let text = serde_json::to_string(stored).map_err(io::Error::other)?;
    crate::atomic_write::write(path, text.as_bytes())
}

/// The saved capture as events. A missing, empty or unreadable file simply means
/// "no Claude data yet": an empty list, never an error for the caller to show.
pub fn read_events(path: &Path) -> Vec<LimitsEvent> {
    let Ok(bytes) = crate::small_file::read(path, crate::small_file::MAX_BYTES) else {
        return Vec::new();
    };
    match serde_json::from_slice::<Stored>(&bytes) {
        Ok(stored) => vec![event_from_stored(&stored)],
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-20T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn temp_file(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "cuantoquedapp-claude-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        folder.join("claude-limits.json")
    }

    // Hand-written; extra fields mimic the rest of the real input and must never be kept.
    const INPUT: &str = r#"{
        "session_id": "secret-session",
        "cwd": "/private/place",
        "model": {"display_name": "x"},
        "rate_limits": {
            "five_hour": {"used_percentage": 23.5, "resets_at": 1738425600},
            "seven_day": {"used_percentage": 41.2, "resets_at": 1738857600}
        }
    }"#;

    #[test]
    fn captures_both_windows() {
        let stored = capture(INPUT.as_bytes(), now()).unwrap();
        assert_eq!(stored.captured_at, "2026-11-20T12:00:00.000Z");
        assert_eq!(stored.five_hour.unwrap().used_percentage, 23.5);
        assert_eq!(stored.seven_day.unwrap().resets_at, 1_738_857_600);
    }

    #[test]
    fn nothing_but_the_limits_is_kept() {
        let stored = capture(INPUT.as_bytes(), now()).unwrap();
        let text = serde_json::to_string(&stored).unwrap();
        assert!(!text.contains("secret-session"));
        assert!(!text.contains("/private/place"));
    }

    #[test]
    fn one_window_alone_is_enough() {
        let input = r#"{"rate_limits":{"seven_day":{"used_percentage":5,"resets_at":1738857600}}}"#;
        let stored = capture(input.as_bytes(), now()).unwrap();
        assert!(stored.five_hour.is_none());
        assert!(stored.seven_day.is_some());
    }

    #[test]
    fn input_without_usable_limits_gives_nothing() {
        for input in [
            "",
            "not json",
            "{}",
            r#"{"rate_limits":{}}"#,
            r#"{"rate_limits":{"five_hour":{"used_percentage":10}}}"#,
            r#"{"rate_limits":{"five_hour":{"resets_at":1738425600}}}"#,
            r#"{"rate_limits":{"five_hour":{"used_percentage":"high","resets_at":1}}}"#,
        ] {
            assert!(capture(input.as_bytes(), now()).is_none(), "{input:?}");
        }
    }

    #[test]
    fn converts_to_the_common_event() {
        let event = event_from_stored(&capture(INPUT.as_bytes(), now()).unwrap());
        assert_eq!(event.rate_limits.limit_id, "claude");
        let primary = event.rate_limits.primary.unwrap();
        assert_eq!(primary.window_minutes, 300);
        assert_eq!(primary.used_percent, 23.5);
        assert_eq!(event.rate_limits.secondary.unwrap().window_minutes, 10080);
        assert!(event.rate_limits.plan_type.is_none());
    }

    #[test]
    fn save_and_read_roundtrip() {
        let path = temp_file("roundtrip");
        let stored = capture(INPUT.as_bytes(), now()).unwrap();
        save(&path, &stored).unwrap();
        let events = read_events(&path);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].timestamp, stored.captured_at);
        // A later save replaces the file completely and leaves no temp file behind.
        save(&path, &stored).unwrap();
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn missing_or_corrupt_file_means_no_data() {
        let path = temp_file("corrupt");
        assert!(read_events(&path).is_empty());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{ not json").unwrap();
        assert!(read_events(&path).is_empty());
        std::fs::write(&path, b"").unwrap();
        assert!(read_events(&path).is_empty());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
