//! Pure selection: each limit keeps its most recent event by real date.
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::{codex::RateLimits, reading::LimitsEvent};

/// Distinct limits kept at most. Codex has a handful (`codex`, `premium`…); a file with
/// thousands of made-up ids is not allowed to grow memory with them.
pub const MAX_LIMITS: usize = 16;

/// Always kept, even with `MAX_LIMITS` reached: the two tools the app is about.
const MAIN_LIMITS: [&str; 2] = ["codex", "claude"];

#[derive(Debug, Clone)]
pub struct LimitsData {
    pub date: DateTime<Utc>,
    pub limits: RateLimits,
}

#[derive(Debug, Clone, Default)]
pub struct Selection {
    pub limits: BTreeMap<String, LimitsData>,
    pub invalid: usize,
}

pub fn select_latest(events: impl IntoIterator<Item = LimitsEvent>) -> Selection {
    let mut output = Selection::default();
    for event in events {
        output.add(event);
    }
    output
}

impl Selection {
    /// One event, with the rule of `select_latest`: invalid ones are only counted, and for
    /// each limit the latest by real date is kept (on ties, the first one).
    pub fn add(&mut self, event: LimitsEvent) {
        let Ok(date) = DateTime::parse_from_rfc3339(&event.timestamp) else {
            self.invalid += 1;
            return;
        };
        let limits = event.rate_limits;
        if limits.limit_id.trim().is_empty()
            || [&limits.primary, &limits.secondary]
                .into_iter()
                .flatten()
                .any(|v| {
                    !v.used_percent.is_finite()
                        || !(0.0..=100.0).contains(&v.used_percent)
                        || v.window_minutes == 0
                        || DateTime::from_timestamp(v.resets_at, 0).is_none()
                })
        {
            self.invalid += 1;
            return;
        }
        self.insert(LimitsData {
            date: date.with_timezone(&Utc),
            limits,
        });
    }

    /// Keeps `data` if it is newer than what is there; on ties the existing one stays
    /// (the reader delivers the most recent files first).
    fn insert(&mut self, data: LimitsData) {
        let id = &data.limits.limit_id;
        let previous = self.limits.get(id);
        if previous.is_none()
            && self.limits.len() >= MAX_LIMITS
            && !MAIN_LIMITS.contains(&id.as_str())
        {
            return;
        }
        if previous.is_none_or(|current| data.date > current.date) {
            self.limits.insert(data.limits.limit_id.clone(), data);
        }
    }

    /// Folds the summary of another source (a file) into this one, as if its events had been
    /// added one by one after the ones already here.
    pub fn merge(&mut self, other: &Selection) {
        self.invalid += other.invalid;
        for data in other.limits.values() {
            self.insert(data.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(id: &str, timestamp: &str) -> LimitsEvent {
        LimitsEvent {
            timestamp: timestamp.into(),
            rate_limits: RateLimits {
                limit_id: id.into(),
                plan_type: None,
                primary: None,
                secondary: None,
                credits: None,
            },
        }
    }

    // SEC-03: made-up ids cannot grow the selection without limit, nor push out the tools.
    #[test]
    fn distinct_limits_are_capped_but_the_main_tools_always_fit() {
        let junk = (0..100).map(|n| event(&format!("junk-{n}"), "2026-11-20T12:00:00Z"));
        let mut selection = select_latest(junk);
        assert_eq!(selection.limits.len(), MAX_LIMITS);
        selection.add(event("codex", "2026-11-20T12:00:00Z"));
        selection.add(event("claude", "2026-11-20T12:00:00Z"));
        assert!(selection.limits.contains_key("codex") && selection.limits.contains_key("claude"));
        // A kept id is still updated with newer data.
        selection.add(event("junk-0", "2026-11-20T13:00:00Z"));
        assert_eq!(
            selection.limits["junk-0"].date.to_rfc3339(),
            "2026-11-20T13:00:00+00:00"
        );
    }
}
