//! Live Codex limits: converts the `account/rateLimits/read` answer of `codex app-server`
//! into the same `LimitsEvent`s that the session files produce, so `select_latest` can pick
//! the freshest one per limit without knowing where it came from.
//!
//! Pure code (no process, no clock): the caller passes the answer and the current time.
//! Only limits fields are read; the account id and everything else in the answer is ignored.
use std::collections::BTreeMap;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    codex::{Credits, RateLimits, Window},
    reading::LimitsEvent,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Answer {
    rate_limits: Option<Snapshot>,
    rate_limits_by_limit_id: Option<BTreeMap<String, Snapshot>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    limit_id: Option<String>,
    plan_type: Option<String>,
    primary: Option<LiveWindow>,
    secondary: Option<LiveWindow>,
    credits: Option<LiveCredits>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveWindow {
    used_percent: f64,
    window_duration_mins: Option<u64>,
    resets_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveCredits {
    has_credits: bool,
    unlimited: bool,
    balance: Option<String>,
}

/// One event per limit in the answer, all stamped with `now`.
/// `Err` only when the answer does not have the expected shape.
pub fn events_from_answer(
    answer: &Value,
    now: DateTime<Utc>,
) -> Result<Vec<LimitsEvent>, serde_json::Error> {
    let answer: Answer = serde_json::from_value(answer.clone())?;
    let timestamp = now.to_rfc3339_opts(SecondsFormat::Millis, true);

    let mut snapshots: BTreeMap<String, Snapshot> =
        answer.rate_limits_by_limit_id.unwrap_or_default();
    // The single `rateLimits` snapshot is the main limit; keep it if the map lacks it.
    if let Some(main) = answer.rate_limits {
        let id = main.limit_id.clone().unwrap_or_else(|| "codex".to_string());
        snapshots.entry(id).or_insert(main);
    }

    Ok(snapshots
        .into_iter()
        .map(|(id, snapshot)| LimitsEvent {
            timestamp: timestamp.clone(),
            rate_limits: convert(id, snapshot),
        })
        .collect())
}

fn convert(key: String, snapshot: Snapshot) -> RateLimits {
    RateLimits {
        limit_id: snapshot.limit_id.unwrap_or(key),
        plan_type: snapshot.plan_type,
        primary: snapshot.primary.and_then(window),
        secondary: snapshot.secondary.and_then(window),
        credits: snapshot.credits.map(|credits| Credits {
            has_credits: credits.has_credits,
            unlimited: credits.unlimited,
            balance: credits.balance.unwrap_or_else(|| "0".to_string()),
        }),
    }
}

/// A window without duration or reset time cannot be shown honestly: it is dropped,
/// never filled with an invented value.
fn window(live: LiveWindow) -> Option<Window> {
    Some(Window {
        used_percent: live.used_percent,
        window_minutes: live.window_duration_mins?,
        resets_at: live.resets_at?,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-20T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    // Hand-written, same shape as the real answer (made-up numbers and ids).
    fn normal_answer() -> Value {
        let snapshot = json!({
            "limitId": "codex",
            "planType": "plus",
            "credits": {"hasCredits": false, "unlimited": false, "balance": "0"},
            "primary": {"usedPercent": 53, "windowDurationMins": 300, "resetsAt": 1795231411},
            "secondary": {"usedPercent": 27, "windowDurationMins": 10080, "resetsAt": 1795736800}
        });
        json!({
            "accountId": "acct-not-read",
            "ordinaryUsageAllowed": true,
            "rateLimits": snapshot.clone(),
            "rateLimitsByLimitId": {"codex": snapshot}
        })
    }

    #[test]
    fn converts_the_normal_answer_to_one_event() {
        let events = events_from_answer(&normal_answer(), now()).unwrap();
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.timestamp, "2026-11-20T12:00:00.000Z");
        let limits = &event.rate_limits;
        assert_eq!(limits.limit_id, "codex");
        assert_eq!(limits.plan_type.as_deref(), Some("plus"));
        let primary = limits.primary.as_ref().unwrap();
        assert_eq!(primary.used_percent, 53.0);
        assert_eq!(primary.window_minutes, 300);
        assert_eq!(primary.resets_at, 1_795_231_411);
        assert_eq!(limits.secondary.as_ref().unwrap().window_minutes, 10080);
        assert_eq!(limits.credits.as_ref().unwrap().balance, "0");
    }

    #[test]
    fn keeps_the_main_snapshot_when_the_map_is_missing() {
        let mut answer = normal_answer();
        answer
            .as_object_mut()
            .unwrap()
            .remove("rateLimitsByLimitId");
        let events = events_from_answer(&answer, now()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].rate_limits.limit_id, "codex");
    }

    #[test]
    fn several_limits_become_several_events() {
        let mut answer = normal_answer();
        answer["rateLimitsByLimitId"]["premium"] = json!({
            "limitId": "premium", "planType": "plus", "primary": null, "secondary": null
        });
        let events = events_from_answer(&answer, now()).unwrap();
        let ids: Vec<&str> = events
            .iter()
            .map(|e| e.rate_limits.limit_id.as_str())
            .collect();
        assert_eq!(ids, ["codex", "premium"]);
        assert!(events[1].rate_limits.primary.is_none());
    }

    #[test]
    fn a_window_without_duration_or_reset_is_dropped_not_invented() {
        let mut answer = normal_answer();
        answer["rateLimits"]["primary"] =
            json!({"usedPercent": 10, "windowDurationMins": null, "resetsAt": 1795231411});
        answer["rateLimitsByLimitId"]["codex"]["secondary"] =
            json!({"usedPercent": 5, "windowDurationMins": 10080, "resetsAt": null});
        let events = events_from_answer(&answer, now()).unwrap();
        // The map entry wins over the main snapshot for the same id.
        assert!(events[0].rate_limits.primary.is_some());
        assert!(events[0].rate_limits.secondary.is_none());
    }

    #[test]
    fn the_key_is_used_when_the_snapshot_has_no_id() {
        let answer =
            json!({"rateLimitsByLimitId": {"codex": {"primary": null, "secondary": null}}});
        let events = events_from_answer(&answer, now()).unwrap();
        assert_eq!(events[0].rate_limits.limit_id, "codex");
    }

    #[test]
    fn credits_without_balance_default_to_zero() {
        let answer = json!({"rateLimits": {"credits": {"hasCredits": true, "unlimited": false}}});
        let events = events_from_answer(&answer, now()).unwrap();
        let credits = events[0].rate_limits.credits.as_ref().unwrap();
        assert!(credits.has_credits);
        assert_eq!(credits.balance, "0");
    }

    #[test]
    fn an_answer_of_the_wrong_shape_is_an_error() {
        assert!(events_from_answer(&json!("nope"), now()).is_err());
        assert!(
            events_from_answer(
                &json!({"rateLimits": {"primary": {"usedPercent": "high"}}}),
                now()
            )
            .is_err()
        );
    }

    #[test]
    fn an_empty_answer_gives_no_events() {
        assert!(events_from_answer(&json!({}), now()).unwrap().is_empty());
    }

    #[test]
    fn the_account_id_is_never_part_of_the_result() {
        let events = events_from_answer(&normal_answer(), now()).unwrap();
        assert!(!format!("{events:?}").contains("acct-not-read"));
    }
}
