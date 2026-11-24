//! Tests of the whole `claude_usage` module (parsing, locating, running and the source).
use chrono::FixedOffset;

use super::{
    locate::BINARY_NAMES,
    parse::parse_clock,
    run::{BILLING_OVERRIDES, usage_command},
    *,
};
use std::path::Path;

fn santiago() -> FixedOffset {
    FixedOffset::west_opt(3 * 3600).unwrap()
}

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-21T00:37:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

// Hand-written, same shape as the real output (the breakdown section is irrelevant).
const OUTPUT: &str = "You are currently using your subscription to power your Claude Code usage\n\nCurrent session: 43% used · resets Nov 21 at 12:20am (America/Santiago)\nCurrent week (all models): 19% used · resets Nov 26 at 1am (America/Santiago)\n\nWhat's contributing to your usage?\n  79%  Start fresh when you switch tasks\n";

#[test]
fn parses_both_windows() {
    let event = parse_usage(OUTPUT, now(), &santiago()).unwrap();
    let limits = event.rate_limits;
    assert_eq!(limits.limit_id, "claude");
    let session = limits.primary.unwrap();
    assert_eq!((session.used_percent, session.window_minutes), (43.0, 300));
    // 12:20am Santiago (UTC-3) is 03:20 UTC.
    assert_eq!(
        session.resets_at,
        DateTime::parse_from_rfc3339("2026-11-21T03:20:00Z")
            .unwrap()
            .timestamp()
    );
    let week = limits.secondary.unwrap();
    assert_eq!((week.used_percent, week.window_minutes), (19.0, 10080));
    assert_eq!(
        week.resets_at,
        DateTime::parse_from_rfc3339("2026-11-26T04:00:00Z")
            .unwrap()
            .timestamp()
    );
    assert_eq!(event.timestamp, "2026-11-21T00:37:00.000Z");
}

#[test]
fn parses_dates_written_with_a_comma() {
    let text = "Current session: 60% used · resets Nov 21, 12:19am (America/Santiago)\nCurrent week (all models): 21% used · resets Nov 26, 12:59am (America/Santiago)\n";
    let limits = parse_usage(text, now(), &santiago()).unwrap().rate_limits;
    let session = limits.primary.unwrap();
    assert_eq!(session.used_percent, 60.0);
    assert_eq!(
        session.resets_at,
        DateTime::parse_from_rfc3339("2026-11-21T03:19:00Z")
            .unwrap()
            .timestamp()
    );
    let week = limits.secondary.unwrap();
    assert_eq!(week.used_percent, 21.0);
    assert_eq!(
        week.resets_at,
        DateTime::parse_from_rfc3339("2026-11-26T03:59:00Z")
            .unwrap()
            .timestamp()
    );
}

#[test]
fn contributing_percentages_are_not_mistaken_for_limits() {
    let text = "What's contributing to your usage?\n  79%  Start fresh\n";
    assert!(parse_usage(text, now(), &santiago()).is_none());
}

// Real output (21-nov-2026) after the session window reset: with no session running, the
// client prints no reset time. It is 0 % used, like Codex shows its idle window.
#[test]
fn an_idle_session_without_a_reset_time_is_zero_percent() {
    let text = "Current session: 0% used\nCurrent week (all models): 28% used · resets Nov 26 at 12:59am (America/Santiago)\n";
    let limits = parse_usage(text, now(), &santiago()).unwrap().rate_limits;
    let session = limits.primary.expect("the idle session is shown");
    assert_eq!(session.used_percent, 0.0);
    assert_eq!(session.window_minutes, 300);
    assert_eq!(session.resets_at, now().timestamp() + 300 * 60);
    assert_eq!(limits.secondary.unwrap().used_percent, 28.0);
}

#[test]
fn a_used_window_without_a_reset_time_is_dropped() {
    let text = "Current session: 12% used\nCurrent week (all models): 19% used · resets Nov 26 at 1am (X)\n";
    let limits = parse_usage(text, now(), &santiago()).unwrap().rate_limits;
    assert!(
        limits.primary.is_none(),
        "a used window needs its reset time"
    );
    assert!(limits.secondary.is_some());
}

#[test]
fn unknown_or_changed_output_gives_nothing() {
    for text in [
        "",
        "Error: not logged in",
        "Current session: lots used · resets soon",
        "Current session: 43% used · resets Foo 21 at 12:20am (X)",
        "Current session: 43% used · resets Nov 32 at 12:20am (X)",
        "Current session: 43% used · resets Nov 21 at 13:20am (X)",
    ] {
        assert!(parse_usage(text, now(), &santiago()).is_none(), "{text:?}");
    }
}

#[test]
fn clock_parsing_covers_midnight_and_noon() {
    assert_eq!(parse_clock("12:20am"), Some((0, 20)));
    assert_eq!(parse_clock("12pm"), Some((12, 0)));
    assert_eq!(parse_clock("1am"), Some((1, 0)));
    assert_eq!(parse_clock("11:59PM"), Some((23, 59)));
    assert_eq!(parse_clock("0am"), None);
    assert_eq!(parse_clock("1:60am"), None);
    assert_eq!(parse_clock("noon"), None);
}

#[test]
fn a_time_without_a_day_means_the_next_occurrence() {
    let text = "Current session: 10% used · resets 12:20am (X)\n";
    let session = parse_usage(text, now(), &santiago())
        .unwrap()
        .rate_limits
        .primary
        .unwrap();
    assert_eq!(
        session.resets_at,
        DateTime::parse_from_rfc3339("2026-11-21T03:20:00Z")
            .unwrap()
            .timestamp()
    );
    // 9pm already passed today (it is 9:37pm there): tomorrow at 9pm = 00:00 UTC two days on.
    let text = "Current session: 10% used · resets 9pm (X)\n";
    let session = parse_usage(text, now(), &santiago())
        .unwrap()
        .rate_limits
        .primary
        .unwrap();
    assert_eq!(
        session.resets_at,
        DateTime::parse_from_rfc3339("2026-11-22T00:00:00Z")
            .unwrap()
            .timestamp()
    );
}

#[test]
fn a_january_reset_read_in_december_is_next_year() {
    let december = DateTime::parse_from_rfc3339("2026-12-30T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let text = "Current week (all models): 5% used · resets Jan 2 at 1am (X)\n";
    let week = parse_usage(text, december, &Utc)
        .unwrap()
        .rate_limits
        .secondary
        .unwrap();
    assert_eq!(
        week.resets_at,
        DateTime::parse_from_rfc3339("2027-01-02T01:00:00Z")
            .unwrap()
            .timestamp()
    );
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cuantoquedapp-claude-bin-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn touch(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"").unwrap();
}

#[test]
fn finds_the_binary_in_the_documented_order() {
    let home = temp_dir("find");
    let local = home.join(".local/bin").join(BINARY_NAMES[0]);
    let on_path = home.join("bin").join(BINARY_NAMES[0]);
    let extension = home
        .join(".vscode/extensions/anthropic.claude-code-2.1.1-x/resources/native-binary")
        .join(BINARY_NAMES[0]);
    let newer_extension = home
        .join(".vscode/extensions/anthropic.claude-code-2.1.9-x/resources/native-binary")
        .join(BINARY_NAMES[0]);
    touch(&extension);
    touch(&newer_extension);
    let mut locations = Locations {
        home: Some(home.clone()),
        path_dirs: vec![home.join("bin")],
        ..Locations::default()
    };
    assert_eq!(find_claude_binary(&locations), Some(newer_extension));
    touch(&on_path);
    assert_eq!(find_claude_binary(&locations), Some(on_path));
    touch(&local);
    assert_eq!(find_claude_binary(&locations), Some(local));
    locations.override_path = Some(home.join("missing"));
    assert_eq!(find_claude_binary(&locations), None);
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn the_child_never_sees_variables_that_override_the_subscription() {
    let command = usage_command(Path::new("claude"));
    for name in BILLING_OVERRIDES {
        // `None` as value means "removed from the child's environment".
        let removed = command
            .get_envs()
            .any(|(key, value)| key == name && value.is_none());
        assert!(removed, "{name} must be removed for the child");
    }
    let args: Vec<_> = command.get_args().collect();
    assert_eq!(args[..2], ["-p", "/usage"]);
}

#[test]
fn nothing_found_is_none() {
    assert_eq!(find_claude_binary(&Locations::default()), None);
}

#[cfg(unix)]
mod process {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn fake(name: &str, script: &str) -> PathBuf {
        let dir = temp_dir(name);
        let path = dir.join("claude");
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn runs_the_client_with_the_expected_arguments() {
        let binary = fake("args", r#"echo "$@""#);
        let text = run_usage(&binary, Duration::from_secs(5)).unwrap();
        assert_eq!(
            text.trim(),
            "-p /usage --no-session-persistence --output-format text"
        );
        let _ = std::fs::remove_dir_all(binary.parent().unwrap());
    }

    // C1
    #[test]
    fn a_child_that_keeps_the_pipe_open_cannot_block_the_reader() {
        // The script exits right away, but a background child inherits stdout and lives on.
        let pidfile = temp_dir("orphan-pid").join("pid");
        let binary = fake(
            "orphan",
            &format!(
                "echo 'Current session: 43% used · resets Nov 21 at 12:20am (X)'\nsleep 30 &\necho $! > '{}'",
                pidfile.display()
            ),
        );
        let started = Instant::now();
        let text = run_usage(&binary, Duration::from_secs(10)).unwrap();
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
        assert!(
            text.contains("Current session: 43%"),
            "partial output is kept"
        );
        assert!(
            crate::process_tree::test_support::process_is_gone(&pidfile),
            "the leftover child is still running"
        );
        let _ = std::fs::remove_dir_all(binary.parent().unwrap());
        let _ = std::fs::remove_dir_all(pidfile.parent().unwrap());
    }

    #[test]
    fn a_timeout_also_kills_the_children_of_the_client() {
        let pidfile = temp_dir("hung-pid").join("pid");
        let binary = fake(
            "hung",
            &format!("sleep 30 &\necho $! > '{}'\nsleep 30", pidfile.display()),
        );
        assert!(run_usage(&binary, Duration::from_millis(500)).is_none());
        assert!(
            crate::process_tree::test_support::process_is_gone(&pidfile),
            "the child of the hung client is still running"
        );
        let _ = std::fs::remove_dir_all(binary.parent().unwrap());
        let _ = std::fs::remove_dir_all(pidfile.parent().unwrap());
    }

    #[test]
    fn failure_and_timeout_give_none() {
        let failing = fake("fail", "echo oops; exit 1");
        assert!(run_usage(&failing, Duration::from_secs(5)).is_none());
        let slow = fake("slow", "sleep 30");
        let started = Instant::now();
        assert!(run_usage(&slow, Duration::from_millis(300)).is_none());
        assert!(started.elapsed() < Duration::from_secs(5));
        let _ = std::fs::remove_dir_all(failing.parent().unwrap());
        let _ = std::fs::remove_dir_all(slow.parent().unwrap());
    }

    #[test]
    fn the_source_asks_once_and_remembers_the_answer() {
        let dir = temp_dir("source");
        let counter = dir.join("calls");
        let script = format!(
            "echo x >> '{}'\necho 'Current session: 43% used · resets Nov 21 at 12:20am (X)'",
            counter.display()
        );
        let binary = fake("source", &script);
        let mut source = ClaudeUsageSource::with_binary(Some(binary.clone()));
        let first = source.poll(now(), &santiago());
        let second = source.poll(now(), &santiago());
        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        let calls = std::fs::read_to_string(&counter).unwrap();
        assert_eq!(
            calls.lines().count(),
            1,
            "second poll must not start another process"
        );
        let _ = std::fs::remove_dir_all(binary.parent().unwrap());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn without_a_binary_the_source_is_empty() {
        let mut source = ClaudeUsageSource::with_binary(None);
        assert!(source.poll(now(), &santiago()).is_empty());
    }
}
