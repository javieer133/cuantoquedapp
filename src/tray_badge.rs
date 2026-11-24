//! Windows tray presentation: the tray cannot show text, so the percentage is drawn
//! into a 32x32 RGBA icon, with a short tooltip. Pure functions: no UI, no clock.
use chrono::{DateTime, Utc};
use rust_i18n::t;

use crate::{
    i18n::Lang,
    monitor::Report,
    presentation::{REFRESHING_MARK, has_reset, limit_name, window_name},
    settings::{Settings, apply_visibility},
};

/// Icon side in pixels.
pub const SIZE: usize = 32;
/// Windows tooltips hold 127 UTF-16 units; stay well below.
pub const TOOLTIP_MAX_CHARS: usize = 120;

const CORNER_RADIUS: i32 = 5;
const WHITE: [u8; 4] = [255, 255, 255, 255];
const GREEN: [u8; 4] = [27, 127, 59, 255];
const AMBER: [u8; 4] = [180, 83, 9, 255];
const RED: [u8; 4] = [185, 28, 28, 255];
const GRAY: [u8; 4] = [75, 85, 99, 255];

/// Usage at or above this percentage turns the icon amber.
const AMBER_FROM: u8 = 60;
/// Usage at or above this percentage turns the icon red.
const RED_FROM: u8 = 85;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeState {
    /// Used percentage of the primary window (0..=100): Codex's, or Claude's if Codex has none.
    Percent(u8),
    /// The window already reset (or the data is dated in the future): the number would lie.
    Stale,
    /// No data at all.
    Unknown,
}

/// With the default settings (the app itself always passes the user's settings).
#[cfg(test)]
pub fn badge_state(report: &Report, now: DateTime<Utc>) -> BadgeState {
    badge_state_with(report, now, &Settings::default())
}

/// Same rule as `presentation::title`: never show a percentage from an expired window.
/// First visible tool with a bar-window value (Codex, then Claude); the tooltip says which.
pub fn badge_state_with(report: &Report, now: DateTime<Utc>, settings: &Settings) -> BadgeState {
    let view = apply_visibility(report, settings);
    let bar = settings.effective_bar_window().minutes();
    ["codex", "claude"]
        .iter()
        .find_map(|tool| tool_badge(&view, tool, bar, now))
        .unwrap_or(BadgeState::Unknown)
}

fn tool_badge(
    report: &Report,
    tool: &str,
    bar_minutes: u64,
    now: DateTime<Utc>,
) -> Option<BadgeState> {
    let data = report.selection.limits.get(tool)?;
    let window = [&data.limits.primary, &data.limits.secondary]
        .into_iter()
        .flatten()
        .find(|window| window.window_minutes == bar_minutes)?;
    if window.resets_at <= now.timestamp() || data.date > now {
        return Some(BadgeState::Stale);
    }
    Some(BadgeState::Percent(
        window.used_percent.round().clamp(0.0, 100.0) as u8,
    ))
}

/// `Codex: 5 hours 46% · Week 25%` for one tool, or `None` if it has no windows.
/// `⚠ ` goes first when a visible window is at or above the alert threshold.
fn tool_line(
    report: &Report,
    tool: &str,
    now: DateTime<Utc>,
    settings: &Settings,
    lang: Lang,
) -> Option<String> {
    let data = report.selection.limits.get(tool)?;
    let windows: Vec<_> = [&data.limits.primary, &data.limits.secondary]
        .into_iter()
        .flatten()
        .collect();
    if windows.is_empty() {
        return None;
    }
    let parts: Vec<String> = windows
        .iter()
        .map(|window| {
            let name = window_name(window.window_minutes, lang);
            if has_reset(window, now) {
                format!("{name} {REFRESHING_MARK}")
            } else {
                format!("{name} {:.0}%", window.used_percent)
            }
        })
        .collect();
    let alert = windows
        .iter()
        .any(|window| settings.alert.is_triggered(window, now));
    let mut line = format!(
        "{}{}: {}",
        if alert { "⚠ " } else { "" },
        limit_name(tool),
        parts.join(" · ")
    );
    // A reset already shows `↻`; this is for data stamped in the future (a wrong clock).
    if data.date > now {
        line.push_str(&format!(" ({})", t!("note.outdated", locale = lang.code())));
    }
    Some(line)
}

/// With the default settings (the app itself always passes the user's settings).
#[cfg(test)]
pub fn tooltip(report: &Report, now: DateTime<Utc>, lang: Lang) -> String {
    tooltip_with(report, now, &Settings::default(), lang)
}

/// Shown until the first reading arrives.
pub fn tooltip_reading(lang: Lang) -> String {
    t!("tooltip.reading", locale = lang.code()).into()
}

/// Short summary for the tray tooltip, one line per visible tool:
/// `Codex: 5 hours 46% · Week 25%` and `⚠ Claude: 5 hours 80% · Week 8%`.
pub fn tooltip_with(
    report: &Report,
    now: DateTime<Utc>,
    settings: &Settings,
    lang: Lang,
) -> String {
    let view = apply_visibility(report, settings);
    let lines: Vec<String> = ["codex", "claude"]
        .iter()
        .filter_map(|tool| tool_line(&view, tool, now, settings, lang))
        .collect();
    if lines.is_empty() {
        return t!("tooltip.no_data", locale = lang.code()).into();
    }
    truncate_chars(&lines.join("\n"), TOOLTIP_MAX_CHARS)
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut short: String = text.chars().take(max - 1).collect();
    short.push('…');
    short
}

/// Renders the icon as `SIZE * SIZE * 4` RGBA bytes, row by row.
pub fn render(state: BadgeState) -> Vec<u8> {
    let mut pixels = vec![0u8; SIZE * SIZE * 4];
    let background = background_for(state);
    for y in 0..SIZE {
        for x in 0..SIZE {
            if inside_rounded_square(x, y) {
                put(&mut pixels, x, y, background);
            }
        }
    }

    let text = match state {
        BadgeState::Percent(value) => value.min(100).to_string(),
        BadgeState::Stale => "!".to_string(),
        BadgeState::Unknown => "?".to_string(),
    };
    let glyphs: Vec<[u8; 5]> = text.chars().map(glyph).collect();
    // "100" needs three digits: smaller scale so it still fits.
    let scale = if glyphs.len() <= 2 { 4 } else { 2 };
    let gap = 2;
    let width = glyphs.len() * 3 * scale + (glyphs.len() - 1) * gap;
    let left = (SIZE - width) / 2;
    let top = (SIZE - 5 * scale) / 2;

    for (index, rows) in glyphs.iter().enumerate() {
        let glyph_x = left + index * (3 * scale + gap);
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..3 {
                if bits >> (2 - col) & 1 == 1 {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            put(
                                &mut pixels,
                                glyph_x + col * scale + dx,
                                top + row * scale + dy,
                                WHITE,
                            );
                        }
                    }
                }
            }
        }
    }
    pixels
}

fn background_for(state: BadgeState) -> [u8; 4] {
    match state {
        BadgeState::Percent(value) if value >= RED_FROM => RED,
        BadgeState::Percent(value) if value >= AMBER_FROM => AMBER,
        BadgeState::Percent(_) => GREEN,
        BadgeState::Stale | BadgeState::Unknown => GRAY,
    }
}

fn put(pixels: &mut [u8], x: usize, y: usize, color: [u8; 4]) {
    let start = (y * SIZE + x) * 4;
    pixels[start..start + 4].copy_from_slice(&color);
}

/// Square with rounded corners; the corner pixels stay transparent.
fn inside_rounded_square(x: usize, y: usize) -> bool {
    let (x, y) = (x as i32, y as i32);
    let last = SIZE as i32 - 1;
    let r = CORNER_RADIUS;
    let center_x = x.clamp(r, last - r);
    let center_y = y.clamp(r, last - r);
    (x - center_x).pow(2) + (y - center_y).pow(2) <= r * r
}

/// 3x5 pixel font; each row holds 3 bits, most significant bit on the left.
fn glyph(character: char) -> [u8; 5] {
    match character {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b001, 0b001, 0b001],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        '!' => [0b010, 0b010, 0b010, 0b000, 0b010],
        _ => [0b111, 0b001, 0b011, 0b000, 0b010], // '?'
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{
        codex::{RateLimits, Window},
        selection_state::{LimitsData, Selection},
    };

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-20T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn window(percent: f64, resets_at: i64) -> Window {
        Window {
            used_percent: percent,
            window_minutes: 300,
            resets_at,
        }
    }

    fn report(primary: Option<Window>, secondary: Option<Window>, date: &str) -> Report {
        let limits = RateLimits {
            limit_id: "codex".into(),
            plan_type: Some("plus".into()),
            primary,
            secondary,
            credits: None,
        };
        let date = DateTime::parse_from_rfc3339(date)
            .unwrap()
            .with_timezone(&Utc);
        let mut map = BTreeMap::new();
        map.insert("codex".to_string(), LimitsData { date, limits });
        Report {
            selection: Selection {
                limits: map,
                invalid: 0,
            },
            notices: Vec::new(),
        }
    }

    fn pixel(pixels: &[u8], x: usize, y: usize) -> [u8; 4] {
        let start = (y * SIZE + x) * 4;
        pixels[start..start + 4].try_into().unwrap()
    }

    fn white_pixels(pixels: &[u8]) -> usize {
        (0..SIZE * SIZE)
            .filter(|i| pixels[i * 4..i * 4 + 4] == WHITE)
            .count()
    }

    #[test]
    fn has_the_expected_size() {
        assert_eq!(render(BadgeState::Percent(5)).len(), SIZE * SIZE * 4);
    }

    #[test]
    fn corners_are_transparent_and_edges_are_filled() {
        let pixels = render(BadgeState::Percent(5));
        assert_eq!(pixel(&pixels, 0, 0)[3], 0);
        assert_eq!(pixel(&pixels, SIZE - 1, SIZE - 1)[3], 0);
        assert_eq!(pixel(&pixels, 1, 16), GREEN);
    }

    #[test]
    fn background_color_follows_usage_level() {
        for (value, color) in [
            (0, GREEN),
            (19, GREEN),
            (59, GREEN),
            (60, AMBER),
            (84, AMBER),
            (85, RED),
            (100, RED),
        ] {
            assert_eq!(
                pixel(&render(BadgeState::Percent(value)), 1, 16),
                color,
                "{value}%"
            );
        }
        assert_eq!(pixel(&render(BadgeState::Unknown), 1, 16), GRAY);
        assert_eq!(pixel(&render(BadgeState::Stale), 1, 16), GRAY);
    }

    #[test]
    fn digits_are_drawn_in_white() {
        // "1" has 8 lit cells and "8" has 13, each cell 4x4 pixels.
        assert_eq!(white_pixels(&render(BadgeState::Percent(1))), 8 * 16);
        assert_eq!(white_pixels(&render(BadgeState::Percent(8))), 13 * 16);
        // "88" is two eights.
        assert_eq!(white_pixels(&render(BadgeState::Percent(88))), 2 * 13 * 16);
    }

    #[test]
    fn different_values_render_differently_and_same_value_is_stable() {
        assert_ne!(
            render(BadgeState::Percent(5)),
            render(BadgeState::Percent(19))
        );
        assert_ne!(render(BadgeState::Stale), render(BadgeState::Unknown));
        assert_eq!(
            render(BadgeState::Percent(19)),
            render(BadgeState::Percent(19))
        );
    }

    #[test]
    fn one_hundred_fits_with_three_digits() {
        let pixels = render(BadgeState::Percent(100));
        // "1" (8) + "0" (12) + "0" (12) lit cells, each 2x2 pixels at the smaller scale.
        assert_eq!(white_pixels(&pixels), (8 + 12 + 12) * 4);
    }

    #[test]
    fn values_above_100_are_capped() {
        assert_eq!(
            render(BadgeState::Percent(250)),
            render(BadgeState::Percent(100))
        );
    }

    #[test]
    fn state_shows_the_rounded_primary_percentage() {
        let r = report(
            Some(window(18.6, now().timestamp() + 3600)),
            None,
            "2026-11-20T11:30:00Z",
        );
        assert_eq!(badge_state(&r, now()), BadgeState::Percent(19));
    }

    #[test]
    fn state_is_stale_after_the_reset_or_with_future_data() {
        let expired = report(
            Some(window(18.0, now().timestamp() - 1)),
            None,
            "2026-11-20T11:30:00Z",
        );
        assert_eq!(badge_state(&expired, now()), BadgeState::Stale);
        let future = report(
            Some(window(18.0, now().timestamp() + 3600)),
            None,
            "2026-11-20T13:00:00Z",
        );
        assert_eq!(badge_state(&future, now()), BadgeState::Stale);
    }

    #[test]
    fn state_is_unknown_without_codex_or_primary_window() {
        let empty = Report {
            selection: Selection::default(),
            notices: Vec::new(),
        };
        assert_eq!(badge_state(&empty, now()), BadgeState::Unknown);
        // Only a weekly window: the number of the bar is the 5-hour one, so there is none.
        let only_week = report(
            None,
            Some(Window {
                used_percent: 3.0,
                window_minutes: 10080,
                resets_at: now().timestamp() + 60,
            }),
            "2026-11-20T11:30:00Z",
        );
        assert_eq!(badge_state(&only_week, now()), BadgeState::Unknown);
    }

    #[test]
    fn tooltip_summarizes_both_windows() {
        let r = report(
            Some(window(46.0, now().timestamp() + 3600)),
            Some(Window {
                used_percent: 25.0,
                window_minutes: 10080,
                resets_at: now().timestamp() + 86_400,
            }),
            "2026-11-20T11:30:00Z",
        );
        assert_eq!(
            tooltip(&r, now(), Lang::Es),
            "Codex: 5 horas 46% · Semana 25%"
        );
    }

    #[test]
    fn tooltip_in_english() {
        let r = report(
            Some(window(46.0, now().timestamp() + 3600)),
            Some(Window {
                used_percent: 25.0,
                window_minutes: 10080,
                resets_at: now().timestamp() + 86_400,
            }),
            "2026-11-20T11:30:00Z",
        );
        assert_eq!(
            tooltip(&r, now(), Lang::En),
            "Codex: 5 hours 46% · Week 25%"
        );
        let stale = report(
            Some(window(46.0, now().timestamp() - 60)),
            None,
            "2026-11-20T11:30:00Z",
        );
        assert_eq!(tooltip(&stale, now(), Lang::En), "Codex: 5 hours ↻");
        assert_eq!(tooltip_reading(Lang::En), "Codex: reading…");
    }

    #[test]
    fn tooltip_flags_stale_data_and_never_exceeds_the_limit() {
        // A window that reset shows `↻` instead of its old percentage.
        let reset = report(
            Some(window(46.0, now().timestamp() - 5)),
            None,
            "2026-11-20T11:30:00Z",
        );
        assert_eq!(tooltip(&reset, now(), Lang::Es), "Codex: 5 horas ↻");
        // Data from the future (a wrong clock) is flagged as outdated.
        let r = report(
            Some(window(46.0, now().timestamp() + 3600)),
            None,
            "2026-11-20T12:30:00Z",
        );
        let text = tooltip(&r, now(), Lang::Es);
        assert!(text.ends_with("(desactualizado)"), "{text}");
        assert!(text.chars().count() <= TOOLTIP_MAX_CHARS);
        let long = "x".repeat(500);
        let short = truncate_chars(&long, TOOLTIP_MAX_CHARS);
        assert_eq!(short.chars().count(), TOOLTIP_MAX_CHARS);
        assert!(short.ends_with('…'));
    }

    #[test]
    fn tooltip_has_one_line_per_tool_and_the_badge_falls_back_to_claude() {
        let mut r = report(None, None, "2026-11-20T11:30:00Z");
        r.selection.limits.clear();
        let claude = RateLimits {
            limit_id: "claude".into(),
            plan_type: None,
            primary: Some(window(40.0, now().timestamp() + 3600)),
            secondary: Some(Window {
                used_percent: 8.0,
                window_minutes: 10080,
                resets_at: now().timestamp() + 86_400,
            }),
            credits: None,
        };
        r.selection.limits.insert(
            "claude".into(),
            LimitsData {
                date: DateTime::parse_from_rfc3339("2026-11-20T11:30:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
                limits: claude,
            },
        );
        // Only Claude: its line and its number.
        assert_eq!(
            tooltip(&r, now(), Lang::Es),
            "Claude: 5 horas 40% · Semana 8%"
        );
        assert_eq!(badge_state(&r, now()), BadgeState::Percent(40));
        // Both: Codex's number, two lines, still under the limit.
        let both = report(
            Some(window(46.0, now().timestamp() + 3600)),
            None,
            "2026-11-20T11:30:00Z",
        );
        let mut both = both;
        both.selection.limits.insert(
            "claude".into(),
            r.selection.limits.remove("claude").unwrap(),
        );
        assert_eq!(
            tooltip(&both, now(), Lang::Es),
            "Codex: 5 horas 46%\nClaude: 5 horas 40% · Semana 8%"
        );
        assert_eq!(badge_state(&both, now()), BadgeState::Percent(46));
        assert!(tooltip(&both, now(), Lang::Es).chars().count() <= TOOLTIP_MAX_CHARS);
    }

    // CA11
    #[test]
    fn tooltip_and_badge_respect_what_is_hidden_and_mark_the_alert() {
        use crate::settings::{Alert, Choice};
        let mut r = report(
            Some(window(80.0, now().timestamp() + 3600)),
            Some(Window {
                used_percent: 30.0,
                window_minutes: 10080,
                resets_at: now().timestamp() + 86_400,
            }),
            "2026-11-20T11:30:00Z",
        );
        let claude = r.selection.limits["codex"].clone();
        let mut claude = claude;
        claude.limits.limit_id = "claude".into();
        claude.limits.primary.as_mut().unwrap().used_percent = 10.0;
        r.selection.limits.insert("claude".into(), claude);

        let all = Settings::default();
        assert_eq!(
            tooltip_with(&r, now(), &all, Lang::Es),
            "⚠ Codex: 5 horas 80% · Semana 30%\nClaude: 5 horas 10% · Semana 30%"
        );
        assert_eq!(badge_state_with(&r, now(), &all), BadgeState::Percent(80));

        let off = all.apply(Choice::Alert(Alert::Off));
        assert!(!tooltip_with(&r, now(), &off, Lang::Es).contains('⚠'));

        let no_five = all.apply(Choice::ToggleFiveHour);
        assert_eq!(
            tooltip_with(&r, now(), &no_five, Lang::Es),
            "Codex: Semana 30%\nClaude: Semana 30%"
        );
        assert_eq!(
            badge_state_with(&r, now(), &no_five),
            BadgeState::Percent(30)
        );

        let only_claude = all.apply(Choice::ToggleCodex);
        assert_eq!(
            tooltip_with(&r, now(), &only_claude, Lang::Es),
            "Claude: 5 horas 10% · Semana 30%"
        );
        assert_eq!(
            badge_state_with(&r, now(), &only_claude),
            BadgeState::Percent(10)
        );
        assert!(tooltip_with(&r, now(), &all, Lang::Es).chars().count() <= TOOLTIP_MAX_CHARS);
    }

    #[test]
    fn tooltip_without_data_says_so() {
        let empty = Report {
            selection: Selection::default(),
            notices: Vec::new(),
        };
        assert_eq!(tooltip(&empty, now(), Lang::Es), "Codex: sin datos");
    }
}

#[cfg(test)]
mod reader_lost_tests {
    use super::*;
    use crate::{monitor::reader_lost_report, presentation::format_report};

    #[test]
    fn lost_report_has_no_data_and_one_clear_notice() {
        let report = reader_lost_report();
        assert!(report.selection.limits.is_empty());
        assert_eq!(report.notices.len(), 1);
        let now = Utc::now();
        assert_eq!(badge_state(&report, now), BadgeState::Unknown);
        assert_eq!(tooltip(&report, now, Lang::Es), "Codex: sin datos");
        assert_eq!(
            format_report(&report, now, &Utc, Lang::Es),
            "La lectura de Codex se detuvo; reinicia la app"
        );
    }
}
