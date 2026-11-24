//! Strict parser of the `/usage` text: anything it does not understand is `None`, never a guess.
use chrono::{DateTime, Datelike, Duration as ChronoDuration, NaiveDate, TimeZone, Utc};

use crate::{
    claude_limits::LIMIT_ID,
    codex::{RateLimits, Window},
    reading::LimitsEvent,
};

const SESSION_PREFIX: &str = "Current session:";
const WEEK_PREFIX: &str = "Current week (all models):";

/// Parses the `/usage` text. `zone` is the zone the client prints times in (the system one).
pub fn parse_usage<T: TimeZone>(text: &str, now: DateTime<Utc>, zone: &T) -> Option<LimitsEvent> {
    let window = |prefix: &str, minutes: u64| {
        text.lines()
            .find_map(|line| line.trim().strip_prefix(prefix))
            .and_then(|rest| parse_window(rest, minutes, now, zone))
    };
    let primary = window(SESSION_PREFIX, 300);
    let secondary = window(WEEK_PREFIX, 10080);
    if primary.is_none() && secondary.is_none() {
        return None;
    }
    Some(LimitsEvent {
        timestamp: now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        rate_limits: RateLimits {
            limit_id: LIMIT_ID.to_string(),
            plan_type: None,
            primary,
            secondary,
            credits: None,
        },
    })
}

/// `" 43% used · resets Nov 21 at 12:20am (America/Santiago)"`. An idle window (`" 0% used"`,
/// no session running) has no reset time: it is shown as 0 % resetting one full window from
/// now, the way Codex reports its idle window. A used window without a reset time is `None`.
fn parse_window<T: TimeZone>(
    rest: &str,
    minutes: u64,
    now: DateTime<Utc>,
    zone: &T,
) -> Option<Window> {
    let (used, after) = rest.trim().split_once("% used")?;
    let used_percent: f64 = used.trim().parse().ok().filter(|p: &f64| p.is_finite())?;
    if used_percent == 0.0 && after.trim().is_empty() {
        return Some(Window {
            used_percent,
            window_minutes: minutes,
            resets_at: now.timestamp() + minutes as i64 * 60,
        });
    }
    let resets = after.split_once("resets ")?.1;
    let resets = resets.split(" (").next()?.trim();
    Some(Window {
        used_percent,
        window_minutes: minutes,
        resets_at: parse_reset(resets, now, zone)?.timestamp(),
    })
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// `"Nov 21 at 12:20am"`, `"Nov 21, 12:20am"`, `"Nov 26 at 1am"` or just `"12:20am"`
/// (today, or tomorrow if past).
fn parse_reset<T: TimeZone>(text: &str, now: DateTime<Utc>, zone: &T) -> Option<DateTime<Utc>> {
    let split = text.split_once(" at ").or_else(|| text.split_once(", "));
    let (day_part, time_part) = match split {
        Some((day, time)) => (Some(day.trim()), time.trim()),
        None => (None, text.trim()),
    };
    let (hour, minute) = parse_clock(time_part)?;
    let local_now = now.with_timezone(zone);
    let date = match day_part {
        Some(day) => {
            let (month, day) = day.split_once(' ')?;
            let month = MONTHS.iter().position(|name| *name == month)? as u32 + 1;
            let day: u32 = day.trim().parse().ok()?;
            let this_year = at(zone, local_now.year(), month, day, hour, minute)?;
            // A date that already passed by months means next year (e.g. read in December).
            if this_year < now - ChronoDuration::days(180) {
                at(zone, local_now.year() + 1, month, day, hour, minute)?
            } else {
                this_year
            }
        }
        None => {
            let today = at(
                zone,
                local_now.year(),
                local_now.month(),
                local_now.day(),
                hour,
                minute,
            )?;
            if today <= now {
                today + ChronoDuration::days(1)
            } else {
                today
            }
        }
    };
    Some(date)
}

fn at<T: TimeZone>(
    zone: &T,
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
) -> Option<DateTime<Utc>> {
    let date = NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, minute, 0)?;
    Some(
        zone.from_local_datetime(&date)
            .single()?
            .with_timezone(&Utc),
    )
}

/// `"12:20am"`, `"1am"`, `"11pm"` → 24-hour clock.
pub(super) fn parse_clock(text: &str) -> Option<(u32, u32)> {
    let lower = text.to_ascii_lowercase();
    let (digits, pm) = if let Some(digits) = lower.strip_suffix("am") {
        (digits, false)
    } else {
        (lower.strip_suffix("pm")?, true)
    };
    let (hour, minute) = match digits.split_once(':') {
        Some((hour, minute)) => (hour.parse::<u32>().ok()?, minute.parse::<u32>().ok()?),
        None => (digits.parse::<u32>().ok()?, 0),
    };
    if !(1..=12).contains(&hour) || minute > 59 {
        return None;
    }
    Some(((hour % 12) + if pm { 12 } else { 0 }, minute))
}
