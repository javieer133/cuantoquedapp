//! What the user chose to see and when to be alerted.
//!
//! Pure rules plus a tolerant `config.json`: a missing, empty or broken file means "defaults"
//! (field by field), never an error for the caller to show.
use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use crate::{
    codex::Window,
    i18n::Language,
    monitor::Report,
    selection_state::{LimitsData, Selection},
};

pub const FIVE_HOUR_MINUTES: u64 = 300;
pub const WEEK_MINUTES: u64 = 10080;

/// Which window gives the number shown in the menu bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarWindow {
    FiveHour,
    Week,
}

impl BarWindow {
    pub fn minutes(self) -> u64 {
        match self {
            BarWindow::FiveHour => FIVE_HOUR_MINUTES,
            BarWindow::Week => WEEK_MINUTES,
        }
    }
}

/// Usage level that turns on the alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alert {
    Off,
    At50,
    At75,
    At90,
}

impl Alert {
    /// The percentage, or `None` when the alert is off.
    pub fn threshold(self) -> Option<u8> {
        match self {
            Alert::Off => None,
            Alert::At50 => Some(50),
            Alert::At75 => Some(75),
            Alert::At90 => Some(90),
        }
    }

    /// A visible, not yet expired window at or above the threshold.
    pub fn is_triggered(self, window: &Window, now: DateTime<Utc>) -> bool {
        self.threshold().is_some_and(|threshold| {
            window.resets_at > now.timestamp() && window.used_percent >= f64::from(threshold)
        })
    }
}

/// Colors of the panel: follow the system, or force one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Auto,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub show_five_hour: bool,
    pub show_week: bool,
    pub show_codex: bool,
    pub show_claude: bool,
    pub bar_window: BarWindow,
    pub alert: Alert,
    pub theme: Theme,
    pub language: Language,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            show_five_hour: true,
            show_week: true,
            show_codex: true,
            show_claude: true,
            bar_window: BarWindow::FiveHour,
            alert: Alert::At75,
            theme: Theme::Auto,
            language: Language::Auto,
        }
    }
}

/// One thing the menu can change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    ToggleFiveHour,
    ToggleWeek,
    ToggleCodex,
    ToggleClaude,
    Bar(BarWindow),
    Alert(Alert),
    Theme(Theme),
    Language(Language),
}

impl Settings {
    /// R1: turning off the last visible window (or tool) is refused.
    pub fn apply(self, choice: Choice) -> Settings {
        let mut next = self;
        match choice {
            Choice::ToggleFiveHour if self.can_toggle_five_hour() => {
                next.show_five_hour = !self.show_five_hour
            }
            Choice::ToggleWeek if self.can_toggle_week() => next.show_week = !self.show_week,
            Choice::ToggleCodex if self.can_toggle_codex() => next.show_codex = !self.show_codex,
            Choice::ToggleClaude if self.can_toggle_claude() => {
                next.show_claude = !self.show_claude
            }
            Choice::Bar(window) => next.bar_window = window,
            Choice::Alert(alert) => next.alert = alert,
            Choice::Theme(theme) => next.theme = theme,
            Choice::Language(language) => next.language = language,
            _ => {}
        }
        next
    }

    pub fn can_toggle_five_hour(&self) -> bool {
        !self.show_five_hour || self.show_week
    }

    pub fn can_toggle_week(&self) -> bool {
        !self.show_week || self.show_five_hour
    }

    pub fn can_toggle_codex(&self) -> bool {
        !self.show_codex || self.show_claude
    }

    pub fn can_toggle_claude(&self) -> bool {
        !self.show_claude || self.show_codex
    }

    /// The bar choice only matters while both windows are visible.
    pub fn can_choose_bar_window(&self) -> bool {
        self.show_five_hour && self.show_week
    }

    /// R2: the chosen window if visible, otherwise the other one. The choice itself is kept.
    pub fn effective_bar_window(&self) -> BarWindow {
        match (self.bar_window, self.show_five_hour, self.show_week) {
            (BarWindow::FiveHour, false, true) => BarWindow::Week,
            (BarWindow::Week, true, false) => BarWindow::FiveHour,
            (chosen, _, _) => chosen,
        }
    }

    /// Windows of unknown length are always shown: only the two we know can be hidden.
    pub fn window_visible(&self, minutes: u64) -> bool {
        match minutes {
            FIVE_HOUR_MINUTES => self.show_five_hour,
            WEEK_MINUTES => self.show_week,
            _ => true,
        }
    }

    pub fn tool_visible(&self, limit_id: &str) -> bool {
        match limit_id {
            "codex" => self.show_codex,
            "claude" => self.show_claude,
            _ => true,
        }
    }
}

/// CA6: a copy of the report without the hidden tools and windows; the original is untouched.
pub fn apply_visibility(report: &Report, settings: &Settings) -> Report {
    let limits = report
        .selection
        .limits
        .iter()
        .filter(|(id, _)| settings.tool_visible(id))
        .map(|(id, data)| {
            let mut limits = data.limits.clone();
            for slot in [&mut limits.primary, &mut limits.secondary] {
                if slot
                    .as_ref()
                    .is_some_and(|window| !settings.window_visible(window.window_minutes))
                {
                    *slot = None;
                }
            }
            (
                id.clone(),
                LimitsData {
                    date: data.date,
                    limits,
                },
            )
        })
        .collect();
    Report {
        selection: Selection {
            limits,
            invalid: report.selection.invalid,
        },
        notices: report.notices.clone(),
    }
}

// ---- config.json ----

fn parse_field<T>(value: &Value, key: &str, parse: impl Fn(&Value) -> Option<T>) -> Option<T> {
    parse(value.get(key)?)
}

/// Field by field: anything missing or invalid keeps its default; unknown fields are ignored.
pub fn parse(bytes: &[u8]) -> Settings {
    let mut settings = Settings::default();
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return settings;
    };
    let boolean = |value: &Value| value.as_bool();
    if let Some(v) = parse_field(&value, "show_five_hour", boolean) {
        settings.show_five_hour = v;
    }
    if let Some(v) = parse_field(&value, "show_week", boolean) {
        settings.show_week = v;
    }
    if let Some(v) = parse_field(&value, "show_codex", boolean) {
        settings.show_codex = v;
    }
    if let Some(v) = parse_field(&value, "show_claude", boolean) {
        settings.show_claude = v;
    }
    if let Some(v) = parse_field(&value, "bar_window", |v| match v.as_str()? {
        "five_hour" => Some(BarWindow::FiveHour),
        "week" => Some(BarWindow::Week),
        _ => None,
    }) {
        settings.bar_window = v;
    }
    if let Some(v) = parse_field(&value, "alert", |v| match v.as_str()? {
        "off" => Some(Alert::Off),
        "50" => Some(Alert::At50),
        "75" => Some(Alert::At75),
        "90" => Some(Alert::At90),
        _ => None,
    }) {
        settings.alert = v;
    }
    if let Some(v) = parse_field(&value, "theme", |v| match v.as_str()? {
        "auto" => Some(Theme::Auto),
        "light" => Some(Theme::Light),
        "dark" => Some(Theme::Dark),
        _ => None,
    }) {
        settings.theme = v;
    }
    if let Some(v) = parse_field(&value, "language", |v| match v.as_str()? {
        "auto" => Some(Language::Auto),
        "es" => Some(Language::Spanish),
        "en" => Some(Language::English),
        _ => None,
    }) {
        settings.language = v;
    }
    // A file that hides everything would leave nothing to show: repair it (R1).
    if !settings.show_five_hour && !settings.show_week {
        settings.show_five_hour = true;
        settings.show_week = true;
    }
    if !settings.show_codex && !settings.show_claude {
        settings.show_codex = true;
        settings.show_claude = true;
    }
    settings
}

pub fn to_json(settings: &Settings) -> String {
    let bar = match settings.bar_window {
        BarWindow::FiveHour => "five_hour",
        BarWindow::Week => "week",
    };
    let alert = match settings.alert {
        Alert::Off => "off",
        Alert::At50 => "50",
        Alert::At75 => "75",
        Alert::At90 => "90",
    };
    let theme = match settings.theme {
        Theme::Auto => "auto",
        Theme::Light => "light",
        Theme::Dark => "dark",
    };
    let language = match settings.language {
        Language::Auto => "auto",
        Language::Spanish => "es",
        Language::English => "en",
    };
    serde_json::to_string_pretty(&json!({
        "show_five_hour": settings.show_five_hour,
        "show_week": settings.show_week,
        "show_codex": settings.show_codex,
        "show_claude": settings.show_claude,
        "bar_window": bar,
        "alert": alert,
        "theme": theme,
        "language": language,
    }))
    .unwrap_or_default()
}

/// macOS: `~/Library/Application Support/cuantoquedapp/config.json`;
/// Windows: `%APPDATA%\cuantoquedapp\config.json`.
pub fn default_path() -> Option<PathBuf> {
    path_from(|name| std::env::var_os(name), "cuantoquedapp")
}

/// Where the app kept its settings under its previous name ([`crate::legacy`]).
pub fn legacy_path() -> Option<PathBuf> {
    path_from(|name| std::env::var_os(name), crate::legacy::OLD_NAME)
}

fn path_from(env: impl Fn(&str) -> Option<OsString>, folder: &str) -> Option<PathBuf> {
    let non_empty = |name: &str| env(name).filter(|value| !value.is_empty());
    let base = match non_empty("APPDATA") {
        Some(appdata) => PathBuf::from(appdata),
        None => PathBuf::from(non_empty("HOME")?)
            .join("Library")
            .join("Application Support"),
    };
    Some(base.join(folder).join("config.json"))
}

/// Missing, unreadable or oversized file → defaults.
pub fn load(path: &Path) -> Settings {
    crate::small_file::read(path, crate::small_file::MAX_BYTES)
        .map(|bytes| parse(&bytes))
        .unwrap_or_default()
}

/// Atomic write (unique temporary file + rename).
pub fn save(path: &Path, settings: &Settings) -> io::Result<()> {
    crate::atomic_write::write(path, to_json(settings).as_bytes())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::codex::RateLimits;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-21T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    // CA3
    #[test]
    fn defaults_match_the_spec() {
        let settings = Settings::default();
        assert!(settings.show_five_hour && settings.show_week);
        assert!(settings.show_codex && settings.show_claude);
        assert_eq!(settings.bar_window, BarWindow::FiveHour);
        assert_eq!(settings.alert, Alert::At75);
        assert_eq!(settings.alert.threshold(), Some(75));
        assert_eq!(settings.theme, Theme::Auto);
        assert_eq!(settings.language, Language::Auto);
    }

    #[test]
    fn choosing_a_language_keeps_the_rest() {
        let english = Settings::default().apply(Choice::Language(Language::English));
        assert_eq!(english.language, Language::English);
        assert_eq!(
            Settings {
                language: Language::Auto,
                ..english
            },
            Settings::default()
        );
    }

    // CA4
    #[test]
    fn the_last_window_and_the_last_tool_cannot_be_hidden() {
        let s = Settings::default().apply(Choice::ToggleFiveHour);
        assert!(!s.show_five_hour && s.show_week);
        assert_eq!(s.apply(Choice::ToggleWeek), s, "last window stays");
        assert!(!s.can_toggle_week());
        // Turning the hidden one back on is always allowed.
        assert!(s.apply(Choice::ToggleFiveHour).show_five_hour);

        let s = Settings::default().apply(Choice::ToggleCodex);
        assert_eq!(s.apply(Choice::ToggleClaude), s, "last tool stays");
        assert!(!s.can_toggle_claude());
        assert!(s.apply(Choice::ToggleCodex).show_codex);
    }

    // CA5
    #[test]
    fn the_bar_window_follows_visibility_without_losing_the_choice() {
        let mut s = Settings::default().apply(Choice::Bar(BarWindow::Week));
        assert_eq!(s.effective_bar_window(), BarWindow::Week);
        s = s.apply(Choice::ToggleWeek);
        assert_eq!(s.effective_bar_window(), BarWindow::FiveHour);
        assert_eq!(s.bar_window, BarWindow::Week, "the choice is kept");
        assert!(!s.can_choose_bar_window());
        s = s.apply(Choice::ToggleWeek);
        assert_eq!(s.effective_bar_window(), BarWindow::Week);

        let s = Settings::default().apply(Choice::ToggleFiveHour);
        assert_eq!(s.effective_bar_window(), BarWindow::Week);
    }

    #[test]
    fn alert_and_bar_choices_apply() {
        let s = Settings::default().apply(Choice::Alert(Alert::At90));
        assert_eq!(s.alert, Alert::At90);
        assert_eq!(s.apply(Choice::Alert(Alert::Off)).alert.threshold(), None);
    }

    fn window(used: f64, minutes: u64, resets_in: i64) -> Window {
        Window {
            used_percent: used,
            window_minutes: minutes,
            resets_at: now().timestamp() + resets_in,
        }
    }

    // CA8
    #[test]
    fn the_alert_threshold_is_inclusive_and_ignores_expired_windows() {
        let at75 = Alert::At75;
        assert!(at75.is_triggered(&window(75.0, 300, 60), now()));
        assert!(!at75.is_triggered(&window(74.9, 300, 60), now()));
        assert!(at75.is_triggered(&window(100.0, 300, 60), now()));
        assert!(!at75.is_triggered(&window(90.0, 300, 0), now()), "expired");
        assert!(!at75.is_triggered(&window(90.0, 300, -5), now()), "expired");
        assert!(!Alert::Off.is_triggered(&window(100.0, 300, 60), now()));
        assert!(Alert::At50.is_triggered(&window(50.0, 10080, 60), now()));
        assert!(!Alert::At90.is_triggered(&window(89.0, 10080, 60), now()));
    }

    fn report() -> Report {
        let mut limits = BTreeMap::new();
        for (id, plan) in [("codex", Some("plus")), ("claude", None), ("premium", None)] {
            limits.insert(
                id.to_string(),
                LimitsData {
                    date: now(),
                    limits: RateLimits {
                        limit_id: id.to_string(),
                        plan_type: plan.map(str::to_string),
                        primary: Some(window(10.0, 300, 3600)),
                        secondary: Some(window(20.0, 10080, 86400)),
                        credits: None,
                    },
                },
            );
        }
        Report {
            selection: Selection { limits, invalid: 2 },
            notices: vec![crate::monitor::Notice::CodexNotFound],
        }
    }

    // CA6
    #[test]
    fn hidden_tools_and_windows_are_removed_from_a_copy() {
        let original = report();
        let settings = Settings::default()
            .apply(Choice::ToggleFiveHour)
            .apply(Choice::ToggleClaude);
        let view = apply_visibility(&original, &settings);
        assert!(!view.selection.limits.contains_key("claude"));
        assert!(view.selection.limits.contains_key("codex"));
        let codex = &view.selection.limits["codex"].limits;
        assert!(codex.primary.is_none(), "5-hour window hidden");
        assert_eq!(codex.secondary.as_ref().unwrap().window_minutes, 10080);
        assert!(
            view.selection.limits.contains_key("premium"),
            "unknown tools are not hideable"
        );
        assert_eq!(view.selection.invalid, 2);
        assert_eq!(view.notices, [crate::monitor::Notice::CodexNotFound]);
        // The original is untouched.
        assert!(original.selection.limits["claude"].limits.primary.is_some());
        assert!(original.selection.limits["codex"].limits.primary.is_some());
    }

    #[test]
    fn windows_of_unknown_length_stay_visible() {
        let settings = Settings::default().apply(Choice::ToggleFiveHour);
        assert!(settings.window_visible(60));
        assert!(!settings.window_visible(300));
    }

    // CA12
    #[test]
    fn json_roundtrip_keeps_every_field() {
        let settings = Settings::default()
            .apply(Choice::ToggleFiveHour)
            .apply(Choice::ToggleClaude)
            .apply(Choice::Bar(BarWindow::Week))
            .apply(Choice::Alert(Alert::At90))
            .apply(Choice::Theme(Theme::Dark))
            .apply(Choice::Language(Language::Spanish));
        assert_eq!(settings.theme, Theme::Dark);
        assert_eq!(settings.language, Language::Spanish);
        assert_eq!(parse(to_json(&settings).as_bytes()), settings);
        assert_eq!(
            parse(to_json(&Settings::default()).as_bytes()),
            Settings::default()
        );
    }

    #[test]
    fn bad_files_fall_back_to_defaults_field_by_field() {
        let d = Settings::default();
        for bytes in [&b""[..], b"not json", b"[]", b"null", b"{", &[0xff, 0xfe]] {
            assert_eq!(parse(bytes), d, "{bytes:?}");
        }
        // Valid fields are kept; invalid ones fall back; unknown ones are ignored.
        let s = parse(
            br#"{"show_week": false, "show_codex": "yes", "bar_window": "month",
                 "alert": "75.5", "future_option": 1}"#,
        );
        assert!(!s.show_week);
        assert!(s.show_codex, "invalid type keeps the default");
        assert_eq!(s.bar_window, BarWindow::FiveHour);
        assert_eq!(s.alert, Alert::At75);
        assert_eq!(parse(br#"{"alert": "off"}"#).alert, Alert::Off);
        assert_eq!(parse(br#"{"theme": "light"}"#).theme, Theme::Light);
        assert_eq!(parse(br#"{"theme": "sepia"}"#).theme, Theme::Auto);
        assert_eq!(parse(br#"{"language": "en"}"#).language, Language::English);
        assert_eq!(parse(br#"{"language": "fr"}"#).language, Language::Auto);
        assert_eq!(
            parse(br#"{"theme": "dark"}"#).language,
            Language::Auto,
            "old files"
        );
    }

    #[test]
    fn a_file_that_hides_everything_is_repaired() {
        let s = parse(br#"{"show_five_hour": false, "show_week": false, "show_codex": false, "show_claude": false}"#);
        assert!(s.show_five_hour && s.show_week && s.show_codex && s.show_claude);
    }

    fn temp_file(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "cuantoquedapp-settings-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        folder.join("config.json")
    }

    #[test]
    fn save_and_load_use_a_real_file_and_leave_no_temp_file() {
        let path = temp_file("save");
        assert_eq!(load(&path), Settings::default(), "missing file");
        let settings = Settings::default().apply(Choice::Alert(Alert::At50));
        save(&path, &settings).unwrap();
        assert_eq!(load(&path), settings);
        save(&path, &Settings::default()).unwrap();
        assert_eq!(load(&path), Settings::default());
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );
        std::fs::write(&path, b"{ broken").unwrap();
        assert_eq!(load(&path), Settings::default(), "corrupt file");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn the_config_path_is_per_user() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| OsString::from(value))
            }
        };
        assert_eq!(
            path_from(env(&[("HOME", "/Users/x")]), "cuantoquedapp").unwrap(),
            PathBuf::from("/Users/x/Library/Application Support/cuantoquedapp/config.json")
        );
        assert_eq!(
            path_from(
                env(&[("APPDATA", "C:/Roaming"), ("HOME", "/Users/x")]),
                "cuantoquedapp"
            )
            .unwrap(),
            PathBuf::from("C:/Roaming/cuantoquedapp/config.json")
        );
        assert_eq!(
            path_from(env(&[("HOME", "/Users/x")]), crate::legacy::OLD_NAME).unwrap(),
            PathBuf::from("/Users/x/Library/Application Support/monitor-creditos/config.json")
        );
        assert!(path_from(env(&[]), "cuantoquedapp").is_none());
    }
}
