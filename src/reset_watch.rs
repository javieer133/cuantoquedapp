//! Fresh data right after a reset. When a shown window passes its reset time, the reading the
//! app holds is from the old window and says nothing about the new one, so the interface asks
//! the reader for a forced refresh at once (the same one as "Refresh now"), instead of waiting
//! for the next round (up to 5 minutes for Claude).
//!
//! A source may still answer with the old window for a moment, so a few more tries follow,
//! spaced out. Pure code: the caller passes both clocks.
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use crate::{
    monitor::Report,
    presentation::has_reset,
    settings::{Settings, apply_visibility},
};

/// Time between two tries for the same reset.
pub const RETRY_AFTER: Duration = Duration::from_secs(60);
/// Tries for one reset, the first one included. After them, the normal rounds take over.
pub const MAX_TRIES: u8 = 3;

/// The latest reset time already passed among the windows the user shows. `None` when every
/// shown window is current.
///
/// When the data was read does not matter: live data is stamped with the time it was asked
/// for, so a source still answering with the old window right after the reset gives data
/// that looks new but is not. That is the case the retries are for.
pub fn passed_reset(report: &Report, settings: &Settings, now: DateTime<Utc>) -> Option<i64> {
    let view = apply_visibility(report, settings);
    view.selection
        .limits
        .values()
        .flat_map(|data| {
            [&data.limits.primary, &data.limits.secondary]
                .into_iter()
                .flatten()
                .filter(|window| has_reset(window, now))
                .map(|window| window.resets_at)
        })
        .max()
}

/// Remembers which reset was already asked for, and how often.
#[derive(Debug, Default)]
pub struct ResetWatch {
    pending: Option<Pending>,
}

#[derive(Debug)]
struct Pending {
    resets_at: i64,
    tries: u8,
    last: Instant,
}

impl ResetWatch {
    /// Whether to ask for a forced refresh now, given `passed_reset` of the current report.
    pub fn should_refresh(&mut self, passed: Option<i64>, now: Instant) -> bool {
        let Some(resets_at) = passed else {
            self.pending = None;
            return false;
        };
        match &mut self.pending {
            Some(pending) if pending.resets_at == resets_at => {
                if pending.tries >= MAX_TRIES || now.duration_since(pending.last) < RETRY_AFTER {
                    return false;
                }
                pending.tries += 1;
                pending.last = now;
                true
            }
            _ => {
                self.pending = Some(Pending {
                    resets_at,
                    tries: 1,
                    last: now,
                });
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{
        codex::{RateLimits, Window},
        selection_state::{LimitsData, Selection},
        settings::Choice,
    };

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn window(minutes: u64, resets_at: &str) -> Window {
        Window {
            used_percent: 40.0,
            window_minutes: minutes,
            resets_at: at(resets_at).timestamp(),
        }
    }

    /// Claude read at 11:00, with its 5-hour window resetting at 12:00 and its week much later.
    fn report() -> Report {
        let limits = RateLimits {
            limit_id: "claude".into(),
            plan_type: None,
            primary: Some(window(300, "2026-11-20T12:00:00Z")),
            secondary: Some(window(10080, "2026-11-25T12:00:00Z")),
            credits: None,
        };
        let mut map = BTreeMap::new();
        map.insert(
            "claude".to_string(),
            LimitsData {
                date: at("2026-11-20T11:00:00Z"),
                limits,
            },
        );
        Report {
            selection: Selection {
                limits: map,
                invalid: 0,
            },
            notices: Vec::new(),
        }
    }

    #[test]
    fn nothing_passed_before_the_reset_time() {
        let settings = Settings::default();
        assert_eq!(
            passed_reset(&report(), &settings, at("2026-11-20T11:59:59Z")),
            None
        );
    }

    #[test]
    fn the_reset_counts_once_its_time_has_come() {
        let settings = Settings::default();
        let reset = at("2026-11-20T12:00:00Z").timestamp();
        for now in ["2026-11-20T12:00:00Z", "2026-11-20T12:30:00Z"] {
            assert_eq!(passed_reset(&report(), &settings, at(now)), Some(reset));
        }
    }

    #[test]
    fn the_old_window_answered_after_the_reset_is_still_stale() {
        // Asked at 12:01, the source still gave the window that reset at 12:00.
        let mut late = report();
        late.selection.limits.get_mut("claude").unwrap().date = at("2026-11-20T12:01:00Z");
        assert_eq!(
            passed_reset(&late, &Settings::default(), at("2026-11-20T12:01:00Z")),
            Some(at("2026-11-20T12:00:00Z").timestamp())
        );
    }

    #[test]
    fn the_new_window_ends_the_watch() {
        let mut fresh = report();
        let claude = fresh.selection.limits.get_mut("claude").unwrap();
        claude.date = at("2026-11-20T12:01:00Z");
        claude.limits.primary = Some(window(300, "2026-11-20T17:00:00Z"));
        assert_eq!(
            passed_reset(&fresh, &Settings::default(), at("2026-11-20T12:01:00Z")),
            None
        );
    }

    #[test]
    fn a_source_stuck_on_the_old_window_is_asked_at_most_max_tries_times() {
        let start = Instant::now();
        let mut watch = ResetWatch::default();
        let asked = (0..60)
            .filter(|minute| watch.should_refresh(Some(100), start + RETRY_AFTER * *minute))
            .count();
        assert_eq!(asked, usize::from(MAX_TRIES));
    }

    #[test]
    fn hidden_tools_and_windows_do_not_count() {
        let now = at("2026-11-20T12:05:00Z");
        let no_claude = Settings::default().apply(Choice::ToggleClaude);
        assert_eq!(passed_reset(&report(), &no_claude, now), None);
        let no_five_hours = Settings::default().apply(Choice::ToggleFiveHour);
        assert_eq!(passed_reset(&report(), &no_five_hours, now), None);
    }

    #[test]
    fn a_reset_is_asked_at_once_then_retried_a_few_times() {
        let start = Instant::now();
        let mut watch = ResetWatch::default();
        assert!(!watch.should_refresh(None, start));
        assert!(watch.should_refresh(Some(100), start), "first try at once");
        assert!(!watch.should_refresh(Some(100), start + Duration::from_secs(1)));
        assert!(watch.should_refresh(Some(100), start + RETRY_AFTER));
        assert!(!watch.should_refresh(Some(100), start + RETRY_AFTER + Duration::from_secs(30)));
        assert!(watch.should_refresh(Some(100), start + RETRY_AFTER * 2));
        // MAX_TRIES reached: the normal rounds take over.
        assert!(!watch.should_refresh(Some(100), start + RETRY_AFTER * 10));
    }

    #[test]
    fn fresh_data_or_a_new_reset_starts_over() {
        let start = Instant::now();
        let mut watch = ResetWatch::default();
        assert!(watch.should_refresh(Some(100), start));
        // Fresh data arrived: nothing to ask for.
        assert!(!watch.should_refresh(None, start + Duration::from_secs(1)));
        // The same reset showing up again (stale data came back) is asked again at once.
        assert!(watch.should_refresh(Some(100), start + Duration::from_secs(2)));
        // A later reset is a new one.
        assert!(watch.should_refresh(Some(200), start + Duration::from_secs(3)));
    }
}
