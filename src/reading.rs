//! Reads a session file and extracts only the limits events.
//!
//! Privacy: only lines containing `"rate_limits"` are interpreted, and of those only
//! `timestamp` and `rate_limits` are kept. Errors store the line number
//! and category, never line text (`serde`'s message may quote content).

use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
    sync::OnceLock,
};

use memchr::memmem;
use rust_i18n::t;

use crate::{
    bounded_line::{LineRead, MAX_LINE_BYTES, read_line_bounded},
    codex::{Line, RateLimits},
    i18n::Lang,
};

/// A `token_count` event with non-null limits.
#[derive(Debug, Clone, PartialEq)]
pub struct LimitsEvent {
    /// UTC, ISO 8601 text.
    pub timestamp: String,
    pub rate_limits: RateLimits,
}

/// A line that contained `"rate_limits"` but could not be interpreted.
#[derive(Debug, Clone, PartialEq)]
pub struct LineError {
    /// Line number, starting at 1.
    pub line: usize,
    pub reason: LineProblem,
}

/// Why a line could not be read. Never holds any text of the line itself.
#[derive(Debug, Clone, PartialEq)]
pub enum LineProblem {
    TooLong,
    Read(std::io::ErrorKind),
    /// `serde_json`'s error category and column.
    Format {
        category: serde_json::error::Category,
        column: usize,
    },
}

impl LineProblem {
    pub fn text(&self, lang: Lang) -> String {
        let locale = lang.code();
        match self {
            LineProblem::TooLong => t!("line.too_long", locale = locale).into(),
            LineProblem::Read(kind) => t!(
                "line.read_error",
                locale = locale,
                kind = format!("{kind:?}")
            )
            .into(),
            LineProblem::Format { category, column } => t!(
                "line.unexpected",
                locale = locale,
                category = format!("{category:?}"),
                column = column
            )
            .into(),
        }
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Reading {
    pub events: Vec<LimitsEvent>,
    pub errors: Vec<LineError>,
}

/// Reads a session file. It only fails if it cannot be opened; unreadable lines
/// are collected in `Reading::errors` and reading continues.
pub fn read_limits(file: &Path) -> Result<Reading, std::io::Error> {
    let reader = BufReader::new(File::open(file)?);
    Ok(read_from(reader))
}

/// Core of the reading, over any source (a file, or text in tests).
pub fn read_from(reader: impl BufRead) -> Reading {
    read_from_with_limit(reader, MAX_LINE_BYTES)
}

/// Same, with an explicit longest line (a smaller one in tests).
pub(crate) fn read_from_with_limit(mut reader: impl BufRead, max_line: usize) -> Reading {
    let mut reading = Reading::default();
    // Read by bytes: a line can be ~1 MB and is not always valid UTF-8.
    let mut bytes = Vec::new();
    let mut number = 0;

    loop {
        match read_line_bounded(&mut reader, &mut bytes, max_line) {
            Ok(LineRead::Eof) => break,
            Ok(LineRead::Complete) => number += 1,
            Ok(LineRead::TooLong { .. }) => {
                number += 1;
                reading.errors.push(LineError {
                    line: number,
                    reason: LineProblem::TooLong,
                });
                continue;
            }
            Err(error) => {
                reading.errors.push(LineError {
                    line: number + 1,
                    reason: LineProblem::Read(error.kind()),
                });
                break;
            }
        }

        match classify_line(&bytes) {
            Classified::Skip => {}
            Classified::Event(event) => reading.events.push(event),
            Classified::Error(reason) => reading.errors.push(LineError {
                line: number,
                reason,
            }),
        }
    }
    reading
}

/// What one line of a session file is.
#[derive(Debug)]
pub enum Classified {
    /// Not a limits event (most lines): nothing to do.
    Skip,
    Event(LimitsEvent),
    /// It looked like a limits event but could not be read (the reason, without line text).
    Error(LineProblem),
}

/// The text that every limits event contains; searched in the raw bytes, so the great
/// majority of lines (conversation) are never converted to text or parsed.
const MARKER: &[u8] = b"\"rate_limits\"";

fn marker_finder() -> &'static memmem::Finder<'static> {
    static FINDER: OnceLock<memmem::Finder<'static>> = OnceLock::new();
    FINDER.get_or_init(|| memmem::Finder::new(MARKER))
}

/// Classifies one line (as bytes: it can be ~1 MiB and is not always valid UTF-8).
pub fn classify_line(bytes: &[u8]) -> Classified {
    if marker_finder().find(bytes).is_none() {
        return Classified::Skip;
    }
    let text = String::from_utf8_lossy(bytes);
    match parse_line(&text) {
        Ok(Some(event)) => Classified::Event(event),
        Ok(None) => Classified::Skip,
        Err(error) => Classified::Error(LineProblem::Format {
            category: error.classify(),
            column: error.column(),
        }),
    }
}

/// `Ok(None)`: a valid line that is not a limits event (another type or `rate_limits: null`).
fn parse_line(text: &str) -> Result<Option<LimitsEvent>, serde_json::Error> {
    let line: Line = serde_json::from_str(text)?;
    let Some(payload) = line.payload else {
        return Ok(None);
    };
    if payload.kind.as_deref() != Some("token_count") {
        return Ok(None);
    }
    Ok(payload.rate_limits.map(|rate_limits| LimitsEvent {
        timestamp: line.timestamp,
        rate_limits,
    }))
}

/// Reading of in-memory content with an explicit line limit, for tests of other modules.
#[cfg(test)]
pub(crate) fn read_from_with_limit_for_tests(content: &[u8], max_line: usize) -> Reading {
    read_from_with_limit(std::io::Cursor::new(content.to_vec()), max_line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // Hand-written fixtures; no conversation content.
    const NORMAL: &str = r#"{"timestamp":"2026-11-20T00:12:43.462Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex","primary":{"used_percent":17.0,"window_minutes":300,"resets_at":1795150000},"secondary":{"used_percent":3.0,"window_minutes":10080,"resets_at":1795736800},"credits":{"has_credits":false,"unlimited":false,"balance":"0"},"plan_type":"plus"}}}"#;
    const PREMIUM: &str = r#"{"timestamp":"2026-11-20T00:13:00.000Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"premium","primary":null,"secondary":null,"credits":null,"plan_type":"plus"}}}"#;
    const NULL_LIMITS: &str = r#"{"timestamp":"2026-11-20T00:14:00.000Z","type":"event_msg","payload":{"type":"token_count","rate_limits":null}}"#;
    const OTHER: &str = r#"{"timestamp":"2026-11-20T00:15:00.000Z","type":"session_meta","payload":{"type":"otro"}}"#;
    const CORRUPT: &str = r#"{"timestamp":"2026-11-20T00:16:00.000Z","payload":{"rate_limits":{"limit_id":"codex","primary":"#;

    fn read_lines(lines: &[&str]) -> Reading {
        read_from(Cursor::new(lines.join("\n")))
    }

    #[test]
    fn normal_case() {
        let l = read_lines(&[NORMAL]);
        assert!(l.errors.is_empty());
        assert_eq!(l.events.len(), 1);
        assert_eq!(l.events[0].timestamp, "2026-11-20T00:12:43.462Z");
        let primary_window = l.events[0].rate_limits.primary.as_ref().unwrap();
        assert_eq!(primary_window.used_percent, 17.0);
    }

    #[test]
    fn ignores_null_rate_limits_and_other_events() {
        let l = read_lines(&[OTHER, NULL_LIMITS, NORMAL]);
        assert!(l.errors.is_empty());
        assert_eq!(l.events.len(), 1);
    }

    #[test]
    fn two_limits_are_kept_separately() {
        let l = read_lines(&[NORMAL, PREMIUM]);
        let ids: Vec<&str> = l
            .events
            .iter()
            .map(|e| e.rate_limits.limit_id.as_str())
            .collect();
        assert_eq!(ids, ["codex", "premium"]);
        assert!(l.events[1].rate_limits.primary.is_none());
    }

    #[test]
    fn corrupt_line_is_reported_and_reading_continues() {
        let l = read_lines(&[NORMAL, CORRUPT, PREMIUM]);
        assert_eq!(l.events.len(), 2);
        assert_eq!(l.errors.len(), 1);
        assert_eq!(l.errors[0].line, 2);
    }

    #[test]
    fn the_error_does_not_contain_line_text() {
        let l = read_lines(&[CORRUPT]);
        for lang in [Lang::Es, Lang::En] {
            let text = l.errors[0].reason.text(lang);
            assert!(!text.contains("codex"), "{text}");
            assert!(!text.contains("2026-11-20"), "{text}");
        }
    }

    #[test]
    fn non_utf8_bytes_do_not_stop_reading() {
        let mut rows = Vec::new();
        rows.extend_from_slice(b"\xff\xfe basura sin limites\n");
        rows.extend_from_slice(NORMAL.as_bytes());
        let l = read_from(Cursor::new(rows));
        assert!(l.errors.is_empty());
        assert_eq!(l.events.len(), 1);
    }

    #[test]
    fn empty_file() {
        let l = read_from(Cursor::new(Vec::<u8>::new()));
        assert_eq!(l, Reading::default());
    }

    #[test]
    fn missing_file_is_an_open_error() {
        let r = read_limits(Path::new("/ruta/que/no/existe/rollout-x.jsonl"));
        assert!(r.is_err());
    }

    // A4
    #[test]
    fn a_line_over_the_limit_is_reported_and_the_rest_is_still_read() {
        let huge = format!(
            r#"{{"timestamp":"2026-11-20T00:20:00.000Z","payload":{{"rate_limits":"{}"}}}}"#,
            "x".repeat(5000)
        );
        let text = [NORMAL, &huge, PREMIUM].join("\n");
        let reading = read_from_with_limit(Cursor::new(text), 2000);
        assert_eq!(reading.events.len(), 2, "the lines around it are kept");
        assert_eq!(reading.errors.len(), 1);
        assert_eq!(reading.errors[0].line, 2);
        assert_eq!(reading.errors[0].reason, LineProblem::TooLong);
        assert_eq!(
            reading.errors[0].reason.text(Lang::Es),
            "línea demasiado larga; se omitió"
        );
        assert_eq!(
            reading.errors[0].reason.text(Lang::En),
            "line too long; skipped"
        );
    }
}
