//! Types for Codex's internal session format (`~/.codex/sessions/**/rollout-*.jsonl`).
//! Only the fields in use are declared; `serde` ignores the rest.
//! Undocumented format, observed on 2026-11-19/20: it may change.

use serde::Deserialize;

/// One line of the `.jsonl` file.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Line {
    /// UTC, ISO 8601 text (converted to a date in a later step).
    pub timestamp: String,
    /// Other line types may have no `payload`.
    pub payload: Option<Payload>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Payload {
    /// `type` is a reserved word in Rust, hence the rename.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// Only `token_count` events carry it, and it may be `null`.
    pub rate_limits: Option<RateLimits>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RateLimits {
    pub limit_id: String,
    pub plan_type: Option<String>,
    /// `premium` has `primary` and `secondary` set to `null`.
    pub primary: Option<Window>,
    pub secondary: Option<Window>,
    pub credits: Option<Credits>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Window {
    pub used_percent: f64,
    pub window_minutes: u64,
    /// Segundos Unix.
    pub resets_at: i64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Credits {
    pub has_credits: bool,
    pub unlimited: bool,
    /// Arrives as text, not as a number.
    pub balance: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Hand-written fixtures; no conversation content.
    const NORMAL: &str = r#"{"timestamp":"2026-11-20T00:12:43.462Z","type":"event_msg","payload":{"type":"token_count","info":{"x":1},"rate_limits":{"limit_id":"codex","limit_name":null,"primary":{"used_percent":17.0,"window_minutes":300,"resets_at":1795150000},"secondary":{"used_percent":3.0,"window_minutes":10080,"resets_at":1795736800},"credits":{"has_credits":false,"unlimited":false,"balance":"0"},"plan_type":"plus"}}}"#;

    const PREMIUM: &str = r#"{"timestamp":"2026-11-20T00:13:00.000Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"premium","primary":null,"secondary":null,"credits":{"has_credits":false,"unlimited":false,"balance":"0"},"plan_type":"plus"}}}"#;

    const NO_LIMITS: &str = r#"{"timestamp":"2026-11-20T00:14:00.000Z","type":"event_msg","payload":{"type":"token_count","rate_limits":null}}"#;

    const OTHER_EVENT: &str = r#"{"timestamp":"2026-11-20T00:15:00.000Z","type":"session_meta"}"#;

    #[test]
    fn normal_case() {
        let line: Line = serde_json::from_str(NORMAL).unwrap();
        let rl = line.payload.unwrap().rate_limits.unwrap();
        assert_eq!(rl.limit_id, "codex");
        assert_eq!(rl.plan_type.as_deref(), Some("plus"));
        let primary_window = rl.primary.unwrap();
        assert_eq!(primary_window.used_percent, 17.0);
        assert_eq!(primary_window.window_minutes, 300);
        assert_eq!(primary_window.resets_at, 1795150000);
        assert_eq!(rl.secondary.unwrap().window_minutes, 10080);
        assert_eq!(rl.credits.unwrap().balance, "0");
    }

    #[test]
    fn premium_with_null_windows() {
        let line: Line = serde_json::from_str(PREMIUM).unwrap();
        let rl = line.payload.unwrap().rate_limits.unwrap();
        assert_eq!(rl.limit_id, "premium");
        assert!(rl.primary.is_none());
        assert!(rl.secondary.is_none());
    }

    #[test]
    fn rate_limits_null() {
        let line: Line = serde_json::from_str(NO_LIMITS).unwrap();
        assert!(line.payload.unwrap().rate_limits.is_none());
    }

    #[test]
    fn line_without_payload() {
        let line: Line = serde_json::from_str(OTHER_EVENT).unwrap();
        assert!(line.payload.is_none());
    }
}
