//! Deterministic presentation: the caller supplies the time and the time zone.
use chrono::{DateTime, Datelike, TimeZone, Utc};
use rust_i18n::t;

use crate::{
    codex::Window,
    i18n::Lang,
    monitor::Report,
    settings::{Settings, apply_visibility},
};

/// Text from outside (ids, plans, balances, notices) made safe to print in a terminal or a
/// menu: control characters, including the ESC that starts terminal escape sequences, become `?`.
pub fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// Display name of a limit id.
pub fn limit_name(limit_id: &str) -> &str {
    match limit_id {
        "codex" => "Codex",
        "claude" => "Claude",
        other => other,
    }
}

pub fn window_name(minutes: u64, lang: Lang) -> String {
    let locale = lang.code();
    match minutes {
        300 => t!("window.five_hours", locale = locale),
        10080 => t!("window.week", locale = locale),
        n => t!("window.minutes", locale = locale, n = n),
    }
    .into()
}

pub fn bar(percent: f64) -> String {
    let filled = (percent.clamp(0.0, 100.0) / 10.0).round() as usize;
    format!("{}{}", "█".repeat(filled), "░".repeat(10 - filled))
}

pub(crate) fn age(date: DateTime<Utc>, now: DateTime<Utc>, lang: Lang) -> String {
    let locale = lang.code();
    let minutes = (now - date).num_minutes();
    if date > now {
        t!("age.future", locale = locale)
    } else if minutes < 60 {
        t!("age.minutes", locale = locale, n = minutes)
    } else {
        t!("age.hours", locale = locale, n = minutes / 60)
    }
    .into()
}

/// How long until a reset, rounded up to the minute: `in 25 min`, `in 3 h 40 min`, `in 5 d 1 h`.
pub fn remaining(seconds: i64, lang: Lang) -> String {
    let locale = lang.code();
    let minutes = (seconds.max(0) + 59) / 60;
    if minutes < 60 {
        t!("remaining.minutes", locale = locale, m = minutes)
    } else if minutes < 24 * 60 {
        let hours = minutes / 60;
        match minutes % 60 {
            0 => t!("remaining.hours", locale = locale, h = hours),
            rest => t!(
                "remaining.hours_minutes",
                locale = locale,
                h = hours,
                m = rest
            ),
        }
    } else {
        let (days, hours) = (minutes / (24 * 60), (minutes % (24 * 60)) / 60);
        match hours {
            0 => t!("remaining.days", locale = locale, d = days),
            _ => t!("remaining.days_hours", locale = locale, d = days, h = hours),
        }
    }
    .into()
}

fn weekday_name(day: chrono::Weekday, lang: Lang) -> String {
    let locale = lang.code();
    match day {
        chrono::Weekday::Mon => t!("day.mon", locale = locale),
        chrono::Weekday::Tue => t!("day.tue", locale = locale),
        chrono::Weekday::Wed => t!("day.wed", locale = locale),
        chrono::Weekday::Thu => t!("day.thu", locale = locale),
        chrono::Weekday::Fri => t!("day.fri", locale = locale),
        chrono::Weekday::Sat => t!("day.sat", locale = locale),
        chrono::Weekday::Sun => t!("day.sun", locale = locale),
    }
    .into()
}

/// `month0`: 0 for January.
fn month_name(month0: u32, lang: Lang) -> String {
    let locale = lang.code();
    match month0 {
        0 => t!("month.jan", locale = locale),
        1 => t!("month.feb", locale = locale),
        2 => t!("month.mar", locale = locale),
        3 => t!("month.apr", locale = locale),
        4 => t!("month.may", locale = locale),
        5 => t!("month.jun", locale = locale),
        6 => t!("month.jul", locale = locale),
        7 => t!("month.aug", locale = locale),
        8 => t!("month.sep", locale = locale),
        9 => t!("month.oct", locale = locale),
        10 => t!("month.nov", locale = locale),
        _ => t!("month.dec", locale = locale),
    }
    .into()
}

/// Reset time split in what the panel shows in two tones: `("in 3 h 40 min", "today 00:23")`.
/// `None` when the window already reset or the date is invalid.
pub(crate) fn reset_parts<T: TimeZone>(
    window: &Window,
    now: DateTime<Utc>,
    zone: &T,
    lang: Lang,
) -> Option<(String, String)> {
    let locale = lang.code();
    let date = DateTime::from_timestamp(window.resets_at, 0).filter(|date| *date > now)?;
    let local = date.with_timezone(zone);
    let clock_time = format!(
        "{:02}:{:02}",
        chrono::Timelike::hour(&local),
        chrono::Timelike::minute(&local)
    );
    let when = if local.date_naive() == now.with_timezone(zone).date_naive() {
        t!("when.today", locale = locale, time = clock_time)
    } else {
        t!(
            "when.date",
            locale = locale,
            weekday = weekday_name(local.weekday(), lang),
            day = local.day(),
            month = month_name(local.month0(), lang),
            time = clock_time
        )
    };
    Some((remaining((date - now).num_seconds(), lang), when.into()))
}

/// `resets in 3 h 40 min (today 00:23)`, or why there is no reset time to show.
pub(crate) fn reset_text<T: TimeZone>(
    window: &Window,
    now: DateTime<Utc>,
    zone: &T,
    lang: Lang,
) -> String {
    let locale = lang.code();
    if let Some((remaining, when)) = reset_parts(window, now, zone, lang) {
        return t!(
            "reset.text",
            locale = locale,
            remaining = remaining,
            when = when
        )
        .into();
    }
    if DateTime::from_timestamp(window.resets_at, 0).is_none() {
        t!("reset.invalid", locale = locale).into()
    } else {
        t!("reset.expired", locale = locale).into()
    }
}

pub fn format_report<T: TimeZone>(
    report: &Report,
    now: DateTime<Utc>,
    zone: &T,
    lang: Lang,
) -> String {
    let locale = lang.code();
    let mut lines = Vec::new();
    for data in report.selection.limits.values() {
        let limits = &data.limits;
        let name = clean(limit_name(&limits.limit_id));
        let plan = limits
            .plan_type
            .as_deref()
            .map(|p| format!(" ({})", t!("report.plan", locale = locale, plan = clean(p))))
            .unwrap_or_default();
        lines.push(format!("{name}{plan} · {}", age(data.date, now, lang)));
        let used = t!("report.used", locale = locale);
        for window in [&limits.primary, &limits.secondary].into_iter().flatten() {
            lines.push(format!(
                "  {:8} {:3.0}% {used}  {}  {}",
                window_name(window.window_minutes, lang),
                window.used_percent,
                bar(window.used_percent),
                reset_text(window, now, zone, lang)
            ));
        }
        if let Some(credits) = &limits.credits {
            let extra = if credits.unlimited {
                t!("credits.unlimited", locale = locale)
            } else if credits.has_credits {
                t!(
                    "credits.yes_balance",
                    locale = locale,
                    balance = clean(&credits.balance)
                )
            } else {
                t!("credits.no", locale = locale)
            };
            lines.push(format!(
                "  {}",
                t!("credits.line", locale = locale, extra = extra)
            ));
        }
    }
    if lines.is_empty() && report.notices.is_empty() {
        lines.push(t!("report.no_data", locale = locale).into());
    }
    lines.extend(
        report
            .notices
            .iter()
            .map(|notice| clean(&notice.text(lang))),
    );
    lines.join("\n")
}

/// Shown instead of a percentage for a window that already reset, until fresh data arrives.
pub const REFRESHING_MARK: &str = "↻";

/// The window's reset time is valid and already passed: its percentage is from before the
/// reset, so it no longer says anything.
pub fn has_reset(window: &Window, now: DateTime<Utc>) -> bool {
    DateTime::from_timestamp(window.resets_at, 0).is_some_and(|reset| reset <= now)
}

/// Title shown when the reading thread died (see `monitor::reader_lost_report`).
pub fn title_reader_lost(lang: Lang) -> String {
    t!("title.reader_lost", locale = lang.code()).into()
}

/// A remark after the title numbers: ` · outdated`.
fn note(key_text: impl std::fmt::Display) -> String {
    format!(" · {key_text}")
}

/// The title split in parts so each platform can present it: macOS shows one badge per tool
/// before its number, other places use the names.
#[derive(Debug, PartialEq)]
pub struct TitleParts {
    /// Limit ids in display order (`"codex"`, `"claude"`).
    pub tools: Vec<&'static str>,
    /// One value per tool, like `"54%"` or `"?%"`.
    pub values: Vec<String>,
    /// One flag per tool: some visible window of it is at or above the alert threshold.
    pub alerts: Vec<bool>,
    /// Trailing remark (` · old data`, ` · warning`, ` · outdated`), possibly empty.
    pub note: String,
}

/// With the default settings (everything visible, 5-hour number).
pub fn title_parts(report: &Report, now: DateTime<Utc>, lang: Lang) -> TitleParts {
    title_parts_with(report, now, &Settings::default(), lang)
}

/// The title honours what the user shows (tools, bar window) and never reuses an expired %:
/// a tool whose bar window reset shows `↻` instead. A tool with no bar window, or with data
/// dated in the future, is left out; if none is left, one `?%` stands for the first visible
/// tool (flagged `outdated` when its data is from the future).
pub fn title_parts_with(
    report: &Report,
    now: DateTime<Utc>,
    settings: &Settings,
    lang: Lang,
) -> TitleParts {
    let locale = lang.code();
    let view = apply_visibility(report, settings);
    let bar_minutes = settings.effective_bar_window().minutes();
    let mut parts = TitleParts {
        tools: Vec::new(),
        values: Vec::new(),
        alerts: Vec::new(),
        note: String::new(),
    };
    let mut old = false;
    let mut fallback: Option<(&'static str, bool)> = None;
    for tool in ["codex", "claude"] {
        if !settings.tool_visible(tool) {
            continue;
        }
        let Some(data) = view.selection.limits.get(tool) else {
            continue;
        };
        let windows = [&data.limits.primary, &data.limits.secondary];
        let bar = windows
            .into_iter()
            .flatten()
            .find(|window| window.window_minutes == bar_minutes);
        match bar {
            // A window that reset keeps its place with `↻`: fresh data is being asked for.
            Some(window) if data.date <= now && has_reset(window, now) => {
                parts.tools.push(tool);
                parts.values.push(REFRESHING_MARK.into());
                parts.alerts.push(false);
            }
            Some(window) if window.resets_at > now.timestamp() && data.date <= now => {
                parts.tools.push(tool);
                parts.values.push(format!("{:.0}%", window.used_percent));
                parts.alerts.push(
                    windows
                        .into_iter()
                        .flatten()
                        .any(|window| settings.alert.is_triggered(window, now)),
                );
                old |= (now - data.date).num_hours() >= 1;
            }
            Some(_) => fallback = fallback.or(Some((tool, true))),
            None => fallback = fallback.or(Some((tool, false))),
        }
    }
    if parts.tools.is_empty() {
        let (tool, stale) = fallback.unwrap_or_else(|| {
            let first = if settings.show_codex {
                "codex"
            } else {
                "claude"
            };
            (first, false)
        });
        parts.tools.push(tool);
        parts.values.push("?%".into());
        parts.alerts.push(false);
        if stale {
            parts.note = note(t!("note.outdated", locale = locale));
        }
        return parts;
    }
    if old {
        parts.note.push_str(&note(t!("note.old", locale = locale)));
    }
    if !report.notices.is_empty() {
        parts
            .note
            .push_str(&note(t!("note.warning", locale = locale)));
    }
    parts
}

/// Title with names: `Codex 54% · Claude 45%`.
pub fn title(report: &Report, now: DateTime<Utc>, lang: Lang) -> String {
    let parts = title_parts(report, now, lang);
    let names: Vec<String> = parts
        .tools
        .iter()
        .zip(&parts.values)
        .map(|(tool, value)| format!("{} {value}", limit_name(tool)))
        .collect();
    format!("{}{}", names.join(" · "), parts.note)
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;

    use super::*;

    // CA1
    #[test]
    fn remaining_follows_the_spec_table_including_the_edges() {
        for (seconds, expected) in [
            (1, "en 1 min"),
            (60, "en 1 min"),
            (61, "en 2 min"),
            (25 * 60, "en 25 min"),
            (59 * 60, "en 59 min"),
            (59 * 60 + 1, "en 1 h"),
            (60 * 60, "en 1 h"),
            (3 * 3600 + 40 * 60, "en 3 h 40 min"),
            (3 * 3600 + 40 * 60 + 1, "en 3 h 41 min"),
            (23 * 3600 + 59 * 60, "en 23 h 59 min"),
            (23 * 3600 + 59 * 60 + 1, "en 1 d"),
            (24 * 3600, "en 1 d"),
            (24 * 3600 + 1, "en 1 d"),
            (5 * 86400 + 3600, "en 5 d 1 h"),
            (5 * 86400, "en 5 d"),
            (0, "en 0 min"),
            (-30, "en 0 min"),
        ] {
            assert_eq!(remaining(seconds, Lang::Es), expected, "{seconds} s");
        }
    }

    fn window(resets_at: i64) -> Window {
        Window {
            used_percent: 10.0,
            window_minutes: 300,
            resets_at,
        }
    }

    // CA2
    #[test]
    fn reset_text_says_how_long_and_when() {
        let zone = FixedOffset::west_opt(3 * 3600).unwrap();
        let now = DateTime::parse_from_rfc3339("2026-11-20T21:37:00-03:00")
            .unwrap()
            .with_timezone(&Utc);
        let later_today = DateTime::parse_from_rfc3339("2026-11-20T23:20:00-03:00")
            .unwrap()
            .timestamp();
        assert_eq!(
            reset_text(&window(later_today), now, &zone, Lang::Es),
            "se reinicia en 1 h 43 min (hoy 23:20)"
        );
        let in_days = DateTime::parse_from_rfc3339("2026-11-26T01:00:00-03:00")
            .unwrap()
            .timestamp();
        assert_eq!(
            reset_text(&window(in_days), now, &zone, Lang::Es),
            "se reinicia en 5 d 3 h (jue 26-nov 01:00)"
        );
        let past = now.timestamp() - 5;
        assert_eq!(
            reset_text(&window(past), now, &zone, Lang::Es),
            "ya se reinició; dato desactualizado"
        );
        assert_eq!(
            reset_text(&window(now.timestamp()), now, &zone, Lang::Es),
            "ya se reinició; dato desactualizado"
        );
        assert_eq!(
            reset_text(&window(i64::MAX), now, &zone, Lang::Es),
            "fecha de reinicio inválida"
        );
        assert!(reset_parts(&window(past), now, &zone, Lang::Es).is_none());
    }

    use std::collections::BTreeMap;

    use crate::{
        codex::RateLimits,
        selection_state::{LimitsData, Selection},
        settings::{Alert, BarWindow, Choice},
    };

    fn at(date: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(date)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn tool(
        id: &str,
        five: Option<f64>,
        week: Option<f64>,
        now: DateTime<Utc>,
    ) -> (String, LimitsData) {
        let window = |used: Option<f64>, minutes: u64, ahead: i64| {
            used.map(|used| Window {
                used_percent: used,
                window_minutes: minutes,
                resets_at: now.timestamp() + ahead,
            })
        };
        (
            id.to_string(),
            LimitsData {
                date: now,
                limits: RateLimits {
                    limit_id: id.to_string(),
                    plan_type: None,
                    primary: window(five, 300, 3600),
                    secondary: window(week, 10080, 86_400),
                    credits: None,
                },
            },
        )
    }

    fn report(tools: Vec<(String, LimitsData)>) -> Report {
        Report {
            selection: Selection {
                limits: tools.into_iter().collect::<BTreeMap<_, _>>(),
                invalid: 0,
            },
            notices: Vec::new(),
        }
    }

    // CA7
    #[test]
    fn the_bar_uses_the_chosen_window_per_tool() {
        let now = at("2026-11-21T12:00:00Z");
        let r = report(vec![
            tool("codex", Some(54.0), Some(27.0), now),
            tool("claude", Some(45.0), Some(21.0), now),
        ]);
        let five = title_parts_with(&r, now, &Settings::default(), Lang::Es);
        assert_eq!(
            (five.tools.as_slice(), five.values.as_slice()),
            (
                ["codex", "claude"].as_slice(),
                ["54%".to_string(), "45%".to_string()].as_slice()
            )
        );
        let week = Settings::default().apply(Choice::Bar(BarWindow::Week));
        assert_eq!(
            title_parts_with(&r, now, &week, Lang::Es).values,
            ["27%", "21%"]
        );
        // Hiding the week window makes the bar fall back to the 5-hour one.
        let hidden = week.apply(Choice::ToggleWeek);
        assert_eq!(
            title_parts_with(&r, now, &hidden, Lang::Es).values,
            ["54%", "45%"]
        );
    }

    #[test]
    fn hidden_tools_leave_the_bar_and_a_missing_value_is_a_question_mark() {
        let now = at("2026-11-21T12:00:00Z");
        let r = report(vec![
            tool("codex", Some(54.0), Some(27.0), now),
            tool("claude", None, Some(21.0), now),
        ]);
        // Claude has no 5-hour window: it is left out of the bar.
        let parts = title_parts_with(&r, now, &Settings::default(), Lang::Es);
        assert_eq!(
            (parts.tools.as_slice(), parts.values.as_slice()),
            (["codex"].as_slice(), ["54%".to_string()].as_slice())
        );
        // Codex hidden and Claude without the bar window: a single `?%`, for Claude.
        let only_claude = Settings::default().apply(Choice::ToggleCodex);
        let parts = title_parts_with(&r, now, &only_claude, Lang::Es);
        assert_eq!(
            (parts.tools.as_slice(), parts.values.as_slice()),
            (["claude"].as_slice(), ["?%".to_string()].as_slice())
        );
        assert_eq!(parts.note, "");
    }

    // CA9
    #[test]
    fn the_alert_marks_the_tool_even_for_the_window_the_bar_does_not_show() {
        let now = at("2026-11-21T12:00:00Z");
        let r = report(vec![
            tool("codex", Some(10.0), Some(20.0), now),
            tool("claude", Some(10.0), Some(80.0), now),
        ]);
        let parts = title_parts_with(&r, now, &Settings::default(), Lang::Es);
        assert_eq!(parts.alerts, [false, true]);
        // Hidden window: its usage no longer raises the alert.
        let no_week = Settings::default().apply(Choice::ToggleWeek);
        assert_eq!(
            title_parts_with(&r, now, &no_week, Lang::Es).alerts,
            [false, false]
        );
        // Alert off, and a higher threshold.
        let off = Settings::default().apply(Choice::Alert(Alert::Off));
        assert_eq!(
            title_parts_with(&r, now, &off, Lang::Es).alerts,
            [false, false]
        );
        let at90 = Settings::default().apply(Choice::Alert(Alert::At90));
        assert_eq!(
            title_parts_with(&r, now, &at90, Lang::Es).alerts,
            [false, false]
        );
        let at50 = Settings::default().apply(Choice::Alert(Alert::At50));
        assert_eq!(
            title_parts_with(&r, now, &at50, Lang::Es).alerts,
            [false, true]
        );
    }

    // A3
    #[test]
    fn terminal_control_characters_never_reach_the_text_output() {
        let now = at("2026-11-21T12:00:00Z");
        let mut hostile = tool("pre\x1b]52;c;ZXZpbA==\x07mium", Some(10.0), None, now);
        hostile.1.limits.plan_type = Some("pl\x1b[31mus\r".into());
        hostile.1.limits.credits = Some(crate::codex::Credits {
            has_credits: true,
            unlimited: false,
            balance: "5\x1b[2J".into(),
        });
        let mut r = report(vec![hostile]);
        r.notices.push(crate::monitor::Notice::FileUnreadable {
            file: std::path::PathBuf::from("aviso\x1b[0m\x07 raro"),
            kind: std::io::ErrorKind::NotFound,
        });
        let text = format_report(&r, now, &Utc, Lang::Es);
        assert!(
            text.chars().all(|c| c == '\n' || !c.is_control()),
            "{text:?}"
        );
        assert!(text.contains("pre?]52;c;ZXZpbA==?mium"));
        assert!(text.contains("saldo 5?[2J"));
        assert_eq!(clean("normal text ñ ✓ 54%"), "normal text ñ ✓ 54%");
    }
}
