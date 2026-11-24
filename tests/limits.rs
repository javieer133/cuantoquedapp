use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
use cuantoquedapp::{
    i18n::Lang,
    monitor::{self, Report},
    presentation::{bar, format_report, title, window_name},
    reading::read_from,
    selection_state::select_latest,
};

/// Reading with a fixed "today", so the 30-day folder limit does not depend on the clock.
fn query(folder: &std::path::Path) -> Report {
    monitor::query(folder, NaiveDate::from_ymd_opt(2026, 11, 21).unwrap())
}

fn query_with(folder: &std::path::Path, live: Vec<cuantoquedapp::reading::LimitsEvent>) -> Report {
    monitor::query_with(folder, live, NaiveDate::from_ymd_opt(2026, 11, 21).unwrap())
}

fn date(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

// Hand-written fixtures: limits fields only, no conversations.
fn event(timestamp: &str, id: &str, percent: u32, reset: &str) -> String {
    format!(
        r#"{{"timestamp":"{timestamp}","type":"event_msg","payload":{{"type":"token_count","rate_limits":{{"limit_id":"{id}","plan_type":"plus","primary":{{"used_percent":{percent},"window_minutes":300,"resets_at":{}}},"secondary":{{"used_percent":3,"window_minutes":10080,"resets_at":{}}},"credits":{{"has_credits":false,"unlimited":false,"balance":"0"}}}}}}}}"#,
        date(reset).timestamp(),
        date("2026-11-26T23:46:00Z").timestamp()
    )
}

struct Folder(PathBuf);
impl Folder {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "monitor-integracion-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn write_file(&self, name: &str, text: &str) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}
impl Drop for Folder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn selection_by_real_date_and_by_id_across_files() {
    let folder = Folder::new();
    folder.write_file(
        "2026/11/01/rollout-antiguo.jsonl",
        &event(
            "2026-11-20T10:00:00-03:00",
            "codex",
            17,
            "2026-11-20T14:00:00Z",
        ),
    );
    folder.write_file(
        "2026/11/20/rollout-nuevo.jsonl",
        &format!(
            "{}\n{}",
            event("2026-11-20T12:30:00Z", "codex", 9, "2026-11-20T14:00:00Z"),
            event("2026-11-20T13:01:00Z", "premium", 5, "2026-11-20T14:00:00Z")
        ),
    );
    let report = query(&folder.0);
    assert_eq!(report.selection.limits.len(), 2);
    assert_eq!(
        report.selection.limits["codex"]
            .limits
            .primary
            .as_ref()
            .unwrap()
            .used_percent,
        17.0
    );
}

#[test]
fn full_output_with_fixed_time_and_zone() {
    let folder = Folder::new();
    folder.write_file(
        "rollout-normal.jsonl",
        &event("2026-11-20T11:35:00Z", "codex", 17, "2026-11-20T13:34:00Z"),
    );
    let report = query(&folder.0);
    let now = date("2026-11-20T12:00:00Z");
    let output = format_report(
        &report,
        now,
        &FixedOffset::west_opt(3 * 3600).unwrap(),
        Lang::Es,
    );
    assert_eq!(
        output,
        "Codex (plan plus) · dato de hace 25 min\n  5 horas   17% usados  ██░░░░░░░░  se reinicia en 1 h 34 min (hoy 10:34)\n  Semana     3% usados  ░░░░░░░░░░  se reinicia en 6 d 11 h (jue 26-nov 20:46)\n  Créditos extra: no"
    );
    assert_eq!(title(&report, now, Lang::Es), "Codex 17%");
}

#[test]
fn full_output_in_english() {
    let folder = Folder::new();
    folder.write_file(
        "rollout-normal.jsonl",
        &event("2026-11-20T11:35:00Z", "codex", 17, "2026-11-20T13:34:00Z"),
    );
    let report = query(&folder.0);
    let now = date("2026-11-20T12:00:00Z");
    let output = format_report(
        &report,
        now,
        &FixedOffset::west_opt(3 * 3600).unwrap(),
        Lang::En,
    );
    assert_eq!(
        output,
        "Codex (plus plan) · data from 25 min ago\n  5 hours   17% used  ██░░░░░░░░  resets in 1 h 34 min (today 10:34)\n  Week       3% used  ░░░░░░░░░░  resets in 6 d 11 h (Thu Nov 26 20:46)\n  Extra credits: no"
    );
    assert_eq!(title(&report, now, Lang::En), "Codex 17%");
}

#[test]
fn expired_reset_in_english() {
    let folder = Folder::new();
    folder.write_file(
        "rollout-reset.jsonl",
        &event("2026-11-20T10:00:00Z", "codex", 17, "2026-11-20T12:00:00Z"),
    );
    let report = query(&folder.0);
    let now = date("2026-11-20T12:00:00Z");
    assert!(format_report(&report, now, &Utc, Lang::En).contains("already reset; data outdated"));
    assert_eq!(title(&report, now, Lang::En), "Codex ↻");
}

#[test]
fn notices_are_worded_in_each_language() {
    let folder = Folder::new();
    let missing = query(&folder.0.join("absent"));
    let es = format_report(&missing, date("2026-11-20T12:00:00Z"), &Utc, Lang::Es);
    let en = format_report(&missing, date("2026-11-20T12:00:00Z"), &Utc, Lang::En);
    assert_eq!(es, "No encontré Codex en este equipo");
    assert_eq!(en, "Codex not found on this computer");
    assert_eq!(
        title(&missing, date("2026-11-20T12:00:00Z"), Lang::En),
        "Codex ?%"
    );
}

#[test]
fn expired_reset_does_not_show_current_percent_in_icon() {
    let folder = Folder::new();
    folder.write_file(
        "rollout-reset.jsonl",
        &event("2026-11-20T10:00:00Z", "codex", 17, "2026-11-20T12:00:00Z"),
    );
    let report = query(&folder.0);
    let now = date("2026-11-20T12:00:00Z");
    assert!(
        format_report(&report, now, &Utc, Lang::Es).contains("ya se reinició; dato desactualizado")
    );
    assert_eq!(title(&report, now, Lang::Es), "Codex ↻");
}

#[test]
fn corrupt_and_invalid_date_do_not_hide_valid_events() {
    let folder = Folder::new();
    folder.write_file(
        "rollout-corrupto.jsonl",
        &format!(
            "{{\"rate_limits\": secreto-que-no-debe-salir\n{}\n{}",
            event("fecha-invalida", "codex", 99, "2026-11-20T14:00:00Z"),
            event("2026-11-20T12:00:00Z", "codex", 17, "2026-11-20T14:00:00Z")
        ),
    );
    let report = query(&folder.0);
    let output = format_report(&report, date("2026-11-20T12:00:00Z"), &Utc, Lang::Es);
    assert!(output.contains("17%"));
    assert!(output.contains("línea 1"));
    assert!(output.contains("fecha o valores inválidos"));
    assert!(!output.contains("secreto-que-no-debe-salir"));
    assert_eq!(
        title(&report, date("2026-11-20T12:00:00Z"), Lang::Es),
        "Codex 17% · aviso"
    );
}

#[test]
fn premium_without_windows_and_extra_credits() {
    let rows = r#"{"timestamp":"2026-11-20T12:00:00Z","payload":{"type":"token_count","rate_limits":{"limit_id":"premium","primary":null,"secondary":null,"credits":{"has_credits":true,"unlimited":false,"balance":"12.5"}}}}"#;
    let selection = select_latest(read_from(rows.as_bytes()).events);
    let report = cuantoquedapp::monitor::Report {
        selection,
        notices: vec![],
    };
    assert_eq!(
        format_report(&report, date("2026-11-20T12:00:00Z"), &Utc, Lang::Es),
        "premium · dato de hace 0 min\n  Créditos extra: sí (saldo 12.5)"
    );
    assert_eq!(
        title(&report, date("2026-11-20T12:00:00Z"), Lang::Es),
        "Codex ?%"
    );
}

#[test]
fn empty_and_missing_folders_have_clear_messages() {
    let folder = Folder::new();
    let now = date("2026-11-20T12:00:00Z");
    assert_eq!(
        format_report(&query(&folder.0), now, &Utc, Lang::Es),
        "Aún no hay datos de límites"
    );
    assert_eq!(
        format_report(&query(&folder.0.join("ausente")), now, &Utc, Lang::Es),
        "No encontré Codex en este equipo"
    );
}

#[test]
fn age_and_future_clock_show_in_title() {
    let folder = Folder::new();
    folder.write_file(
        "rollout-reloj.jsonl",
        &event("2026-11-20T12:00:00Z", "codex", 17, "2026-11-20T18:00:00Z"),
    );
    let report = query(&folder.0);
    let later = date("2026-11-20T14:00:00Z");
    assert!(title(&report, later, Lang::Es).contains("dato antiguo"));
    assert!(format_report(&report, later, &Utc, Lang::Es).contains("dato de hace 2 h"));
    let before = date("2026-11-20T11:00:00Z");
    assert!(title(&report, before, Lang::Es).contains("desactualizado"));
    assert!(format_report(&report, before, &Utc, Lang::Es).contains("fecha futura"));
}

#[test]
fn cli_works_with_fixture_and_injected_time() {
    let folder = Folder::new();
    folder.write_file(
        "rollout-cli.jsonl",
        &event("2026-11-20T12:00:00Z", "codex", 17, "2026-11-20T18:00:00Z"),
    );
    // `CUANTOQUEDAPP_LANG` pins the language, whatever the machine running the tests speaks.
    for (lang, expected) in [("es", "17% usados"), ("en", "17% used")] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_cuantoquedapp-cli"))
            .env("CUANTOQUEDAPP_LANG", lang)
            .arg("--folder")
            .arg(&folder.0)
            .args(["--now", "2026-11-20T12:00:00Z"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{lang}");
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains(expected), "{lang}: {text}");
    }
}

#[test]
fn cli_errors_follow_the_language() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cuantoquedapp-cli"))
        .env("CUANTOQUEDAPP_LANG", "en")
        .arg("--carpeta")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(
        text.starts_with("Unknown or repeated argument: --carpeta\nUsage: cuantoquedapp-cli"),
        "{text}"
    );
}

#[test]
fn names_and_bars_have_stable_length() {
    assert_eq!(window_name(90, Lang::Es), "90 min");
    for percent in [0., 3., 17., 100.] {
        assert_eq!(bar(percent).chars().count(), 10);
    }
}

#[test]
fn out_of_range_percent_is_not_shown() {
    let rows = event("2026-11-20T12:00:00Z", "codex", 117, "2026-11-20T18:00:00Z");
    let selection = select_latest(read_from(rows.as_bytes()).events);
    assert!(selection.limits.is_empty());
    assert_eq!(selection.invalid, 1);
}

fn claude_events(
    five_hour_resets: &str,
    captured: &str,
) -> Vec<cuantoquedapp::reading::LimitsEvent> {
    let input = format!(
        r#"{{"rate_limits":{{"five_hour":{{"used_percentage":40.4,"resets_at":{}}},"seven_day":{{"used_percentage":8,"resets_at":{}}}}}}}"#,
        date(five_hour_resets).timestamp(),
        date("2026-11-26T20:00:00Z").timestamp()
    );
    let stored = cuantoquedapp::claude_limits::capture(input.as_bytes(), date(captured)).unwrap();
    vec![cuantoquedapp::claude_limits::event_from_stored(&stored)]
}

#[test]
fn claude_joins_codex_in_report_and_title() {
    let folder = Folder::new();
    folder.write_file(
        "rollout-normal.jsonl",
        &event("2026-11-20T11:35:00Z", "codex", 17, "2026-11-20T13:34:00Z"),
    );
    let report = query_with(
        &folder.0,
        claude_events("2026-11-20T15:00:00Z", "2026-11-20T11:50:00Z"),
    );
    let now = date("2026-11-20T12:00:00Z");
    let output = format_report(&report, now, &Utc, Lang::Es);
    assert!(output.contains("Claude · dato de hace 10 min"));
    assert!(output.contains("5 horas   40% usados"));
    assert!(output.contains("Codex (plan plus)"));
    assert_eq!(title(&report, now, Lang::Es), "Codex 17% · Claude 40%");
}

#[test]
fn an_expired_claude_window_stays_in_the_title_as_refreshing() {
    let folder = Folder::new();
    folder.write_file(
        "rollout-normal.jsonl",
        &event("2026-11-20T11:35:00Z", "codex", 17, "2026-11-20T13:34:00Z"),
    );
    let report = query_with(
        &folder.0,
        claude_events("2026-11-20T11:55:00Z", "2026-11-20T11:50:00Z"),
    );
    assert_eq!(
        title(&report, date("2026-11-20T12:00:00Z"), Lang::Es),
        "Codex 17% · Claude ↻"
    );
}

#[test]
fn claude_alone_still_gives_a_title() {
    let report = query_with(
        &folder_missing(),
        claude_events("2026-11-20T15:00:00Z", "2026-11-20T11:50:00Z"),
    );
    assert_eq!(
        title(&report, date("2026-11-20T12:00:00Z"), Lang::Es),
        "Claude 40%"
    );
}

fn folder_missing() -> PathBuf {
    std::env::temp_dir().join("cuantoquedapp-does-not-exist")
}

#[test]
fn title_parts_keep_the_same_order_as_the_names_title() {
    use cuantoquedapp::presentation::title_parts;
    let folder = Folder::new();
    folder.write_file(
        "rollout-normal.jsonl",
        &event("2026-11-20T10:35:00Z", "codex", 17, "2026-11-20T13:34:00Z"),
    );
    let report = query_with(
        &folder.0,
        claude_events("2026-11-20T15:00:00Z", "2026-11-20T11:50:00Z"),
    );
    let now = date("2026-11-20T12:00:00Z");
    let parts = title_parts(&report, now, Lang::Es);
    assert_eq!(parts.tools, ["codex", "claude"]);
    assert_eq!(parts.values, ["17%", "40%"]);
    assert_eq!(parts.note, " · dato antiguo");
    assert_eq!(
        title(&report, now, Lang::Es),
        "Codex 17% · Claude 40% · dato antiguo"
    );
}

#[test]
fn title_parts_without_data_show_a_question_mark_for_codex() {
    use cuantoquedapp::presentation::title_parts;
    let report = query(&folder_missing());
    let parts = title_parts(&report, date("2026-11-20T12:00:00Z"), Lang::Es);
    assert_eq!(parts.tools, ["codex"]);
    assert_eq!(parts.values, ["?%"]);
}
