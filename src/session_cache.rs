//! Incremental reading of session files.
//!
//! For each file the cache remembers how far it was read and a summary of what was read
//! (the latest value of each limit, how many events were invalid, and the first errors).
//! A new reading only processes the bytes appended since, so the cost no longer grows with
//! the size of the file. The summary uses the same rule as `select_latest`, so the result is
//! the same as reading the whole file again.
use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use crate::{
    bounded_line::{LineRead, MAX_LINE_BYTES, read_line_bounded},
    reading::{Classified, LineError, LineProblem, classify_line},
    selection_state::Selection,
};

/// How many leading bytes identify a file (to notice that it was replaced).
const HEAD_BYTES: usize = 256;
/// Errors kept per file: the interface shows three and counts the rest.
pub const ERRORS_KEPT: usize = 3;

/// What was read from one file.
#[derive(Debug, Clone, Default)]
pub struct FileSummary {
    /// Latest valid value of each limit in the file, and how many events were invalid.
    pub selection: Selection,
    /// The first `ERRORS_KEPT` format errors, with their line numbers.
    pub errors: Vec<LineError>,
    /// All format errors found, including the ones not kept.
    pub errors_total: usize,
}

impl FileSummary {
    fn apply(&mut self, line: usize, outcome: Classified) {
        match outcome {
            Classified::Skip => {}
            Classified::Event(event) => self.selection.add(event),
            Classified::Error(reason) => self.push_error(line, reason),
        }
    }

    fn push_error(&mut self, line: usize, reason: LineProblem) {
        self.errors_total += 1;
        if self.errors.len() < ERRORS_KEPT {
            self.errors.push(LineError { line, reason });
        }
    }
}

#[derive(Default)]
struct FileState {
    /// Bytes consumed: everything before it is in `summary`.
    offset: u64,
    /// Complete lines consumed.
    lines: usize,
    head: Vec<u8>,
    summary: FileSummary,
}

#[derive(Default)]
pub struct SessionCache {
    files: HashMap<PathBuf, FileState>,
    bytes_read: u64,
}

impl SessionCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Content bytes read so far (a measure for tests and diagnostics).
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    /// Forgets the files that are not in `paths` any more.
    pub fn keep_only(&mut self, paths: &[PathBuf]) {
        self.files.retain(|path, _| paths.contains(path));
    }

    pub fn tracked_files(&self) -> usize {
        self.files.len()
    }

    /// Summary of the whole file, reading only what was appended since the last call.
    /// Fails only if the file cannot be opened or its first bytes cannot be read.
    pub fn read(&mut self, path: &Path) -> io::Result<FileSummary> {
        self.read_with_limit(path, MAX_LINE_BYTES)
    }

    fn read_with_limit(&mut self, path: &Path, max_line: usize) -> io::Result<FileSummary> {
        let mut file = File::open(path)?;
        let length = file.metadata()?.len();
        let mut head = vec![0u8; HEAD_BYTES.min(usize::try_from(length).unwrap_or(HEAD_BYTES))];
        file.read_exact(&mut head)?;
        self.bytes_read += head.len() as u64;

        let state = self.files.entry(path.to_path_buf()).or_default();
        // R2: shorter than what was read, or another beginning: it is a different file.
        if length < state.offset || !head.starts_with(&state.head) {
            *state = FileState::default();
        }
        state.head = head;
        // R3: nothing new.
        if length == state.offset {
            return Ok(state.summary.clone());
        }

        file.seek(SeekFrom::Start(state.offset))?;
        let mut reader = BufReader::new(file);
        let mut bytes = Vec::new();
        // What an unfinished last line would add; it is not kept (R1).
        let mut transient = None::<(usize, Classified)>;
        loop {
            let number = state.lines + 1;
            match read_line_bounded(&mut reader, &mut bytes, max_line) {
                Ok(LineRead::Eof) => break,
                Ok(LineRead::Complete) => {
                    self.bytes_read += bytes.len() as u64;
                    let outcome = classify_line(&bytes);
                    if bytes.ends_with(b"\n") {
                        state.summary.apply(number, outcome);
                        state.offset += bytes.len() as u64;
                        state.lines += 1;
                    } else {
                        transient = Some((number, outcome));
                    }
                }
                Ok(LineRead::TooLong {
                    bytes: size,
                    terminated,
                }) => {
                    let reason = LineProblem::TooLong;
                    if terminated {
                        state.summary.push_error(number, reason);
                        state.offset += size as u64;
                        state.lines += 1;
                    } else {
                        transient = Some((number, Classified::Error(reason)));
                    }
                }
                Err(error) => {
                    transient = Some((number, Classified::Error(LineProblem::Read(error.kind()))));
                    break;
                }
            }
        }
        let mut summary = state.summary.clone();
        if let Some((number, outcome)) = transient {
            summary.apply(number, outcome);
        }
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::{
        reading::{LimitsEvent, read_from},
        selection_state::select_latest,
    };

    fn dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("cuantoquedapp-cache-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn event(timestamp: &str, id: &str, used: f64) -> String {
        format!(
            r#"{{"timestamp":"{timestamp}","type":"event_msg","payload":{{"type":"token_count","rate_limits":{{"limit_id":"{id}","plan_type":"plus","primary":{{"used_percent":{used},"window_minutes":300,"resets_at":1795150000}},"secondary":null,"credits":null}}}}}}"#
        )
    }

    /// The result of reading all the content again, with the code that existed before.
    fn full(content: &[u8], max_line: usize) -> FileSummary {
        let reading = crate::reading::read_from_with_limit_for_tests(content, max_line);
        let mut summary = FileSummary::default();
        let mut selection = Selection::default();
        for event in reading.events {
            selection.add(event);
        }
        summary.selection = selection;
        summary.errors_total = reading.errors.len();
        summary.errors = reading.errors.into_iter().take(ERRORS_KEPT).collect();
        summary
    }

    fn same(left: &FileSummary, right: &FileSummary) -> bool {
        left.selection.invalid == right.selection.invalid
            && left.errors == right.errors
            && left.errors_total == right.errors_total
            && left.selection.limits.len() == right.selection.limits.len()
            && left.selection.limits.iter().all(|(id, data)| {
                right
                    .selection
                    .limits
                    .get(id)
                    .is_some_and(|other| other.date == data.date && other.limits == data.limits)
            })
    }

    /// A corpus with everything that matters: ties, invalid values, corrupt and long lines,
    /// other event types, a line without the marker, two limits.
    fn corpus() -> Vec<String> {
        vec![
            event("2026-11-20T10:00:00.000Z", "codex", 10.0),
            r#"{"type":"session_meta","payload":{"type":"other"}}"#.to_string(),
            event("2026-11-20T11:00:00.000Z", "codex", 20.0),
            event("2026-11-20T11:00:00.000Z", "codex", 99.0), // tie: the first stays
            event("not-a-date", "codex", 30.0),               // invalid timestamp
            event("2026-11-20T12:00:00.000Z", "", 40.0),      // empty id: invalid
            event("2026-11-20T12:00:00.000Z", "codex", 150.0), // out of range: invalid
            r#"{"timestamp":"2026-11-20T12:30:00.000Z","payload":{"rate_limits":{"limit_id":"codex","primary":"#.to_string(), // corrupt
            event("2026-11-20T13:00:00.000Z", "premium", 5.0),
            format!(
                r#"{{"timestamp":"2026-11-20T13:30:00.000Z","payload":{{"rate_limits":"{}"}}}}"#,
                "x".repeat(3000)
            ), // too long for the small limit used in tests
            event("2026-11-20T14:00:00.000Z", "codex", 60.0),
            "plain text line".to_string(),
            r#"{"rate_limits": 1}"#.to_string(), // marker but wrong shape
            event("2026-11-20T15:00:00.000Z", "codex", 70.0),
        ]
    }

    // I1: every way of cutting the content gives the same as reading it all.
    #[test]
    fn appending_in_any_chunks_gives_the_same_as_reading_everything() {
        let content = corpus().join("\n") + "\n";
        let bytes = content.as_bytes();
        for chunk in [1usize, 7, 64, 500, 4096, bytes.len()] {
            let folder = dir("chunks");
            let path = folder.join("rollout-a.jsonl");
            std::fs::write(&path, b"").unwrap();
            let mut cache = SessionCache::new();
            let mut written = 0;
            while written < bytes.len() {
                let end = (written + chunk).min(bytes.len());
                let mut file = std::fs::OpenOptions::new()
                    .append(true)
                    .open(&path)
                    .unwrap();
                file.write_all(&bytes[written..end]).unwrap();
                written = end;
                let got = cache.read_with_limit(&path, 2000).unwrap();
                assert!(
                    same(&got, &full(&bytes[..written], 2000)),
                    "chunk {chunk}, written {written}"
                );
            }
            let _ = std::fs::remove_dir_all(folder);
        }
    }

    // I2
    #[test]
    fn a_second_reading_without_new_bytes_reads_no_content() {
        let folder = dir("idle");
        let path = folder.join("rollout-a.jsonl");
        std::fs::write(&path, corpus().join("\n") + "\n").unwrap();
        let mut cache = SessionCache::new();
        let first = cache.read_with_limit(&path, 2000).unwrap();
        let after_first = cache.bytes_read();
        assert!(after_first > 1000);
        let second = cache.read_with_limit(&path, 2000).unwrap();
        assert!(same(&first, &second));
        assert!(
            cache.bytes_read() - after_first <= HEAD_BYTES as u64,
            "only the identifying head is read"
        );
        // Appending one line reads only that line (plus the head).
        let line = event("2026-11-21T00:00:00.000Z", "codex", 71.0) + "\n";
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(line.as_bytes()).unwrap();
        let before = cache.bytes_read();
        let third = cache.read_with_limit(&path, 2000).unwrap();
        assert_eq!(
            third.selection.limits["codex"]
                .limits
                .primary
                .as_ref()
                .unwrap()
                .used_percent,
            71.0
        );
        assert!(cache.bytes_read() - before <= line.len() as u64 + HEAD_BYTES as u64);
        let _ = std::fs::remove_dir_all(folder);
    }

    // I3
    #[test]
    fn a_truncated_or_replaced_file_is_read_again_from_the_start() {
        let folder = dir("replace");
        let path = folder.join("rollout-a.jsonl");
        let old = event("2026-11-20T10:00:00.000Z", "codex", 10.0) + "\n";
        std::fs::write(&path, old.repeat(5)).unwrap();
        let mut cache = SessionCache::new();
        cache.read(&path).unwrap();
        // Truncated and rewritten with something shorter.
        let short = event("2026-11-21T10:00:00.000Z", "codex", 33.0) + "\n";
        std::fs::write(&path, &short).unwrap();
        let got = cache.read(&path).unwrap();
        assert!(same(&got, &full(short.as_bytes(), MAX_LINE_BYTES)));
        // Replaced by a longer file with another beginning.
        let longer = (event("2026-11-22T10:00:00.000Z", "codex", 44.0) + "\n").repeat(8);
        std::fs::write(&path, &longer).unwrap();
        let got = cache.read(&path).unwrap();
        assert!(same(&got, &full(longer.as_bytes(), MAX_LINE_BYTES)));
        assert_eq!(
            got.selection.limits["codex"]
                .limits
                .primary
                .as_ref()
                .unwrap()
                .used_percent,
            44.0
        );
        let _ = std::fs::remove_dir_all(folder);
    }

    // I4
    #[test]
    fn an_unfinished_last_line_is_not_consumed_until_it_has_a_newline() {
        let folder = dir("partial");
        let path = folder.join("rollout-a.jsonl");
        let complete = event("2026-11-20T10:00:00.000Z", "codex", 10.0);
        let second = event("2026-11-20T11:00:00.000Z", "codex", 20.0);
        // First line done, second one cut in the middle.
        let cut = second.len() / 2;
        std::fs::write(&path, format!("{complete}\n{}", &second[..cut])).unwrap();
        let mut cache = SessionCache::new();
        let got = cache.read(&path).unwrap();
        assert_eq!(
            got.selection.limits["codex"]
                .limits
                .primary
                .as_ref()
                .unwrap()
                .used_percent,
            10.0
        );
        assert_eq!(
            got.errors_total, 1,
            "like a full reading, the broken tail is reported"
        );
        // The rest arrives, with no newline yet: it is a complete JSON, so it counts.
        std::fs::write(&path, format!("{complete}\n{second}")).unwrap();
        let got = cache.read(&path).unwrap();
        assert_eq!(
            got.selection.limits["codex"]
                .limits
                .primary
                .as_ref()
                .unwrap()
                .used_percent,
            20.0
        );
        assert_eq!(got.errors_total, 0);
        // And the newline: the line is now consumed once, with the right line numbers after it.
        std::fs::write(
            &path,
            format!("{complete}\n{second}\nnot json but has \"rate_limits\"\n"),
        )
        .unwrap();
        let got = cache.read(&path).unwrap();
        assert_eq!(got.errors.len(), 1);
        assert_eq!(got.errors[0].line, 3);
        let _ = std::fs::remove_dir_all(folder);
    }

    // I5: on top of I1, the combination of files equals one reading of all the events.
    #[test]
    fn combining_file_summaries_equals_selecting_over_all_the_events() {
        let a = corpus()[..7].join("\n") + "\n";
        let b = corpus()[7..].join("\n") + "\n";
        let folder = dir("combine");
        let (pa, pb) = (
            folder.join("rollout-a.jsonl"),
            folder.join("rollout-b.jsonl"),
        );
        std::fs::write(&pa, &a).unwrap();
        std::fs::write(&pb, &b).unwrap();
        let mut cache = SessionCache::new();
        let mut combined = Selection::default();
        for path in [&pa, &pb] {
            combined.merge(&cache.read_with_limit(path, 2000).unwrap().selection);
        }
        // Reference: every event of both files, in order, through `select_latest`.
        let all: Vec<LimitsEvent> = [a.as_bytes(), b.as_bytes()]
            .iter()
            .flat_map(|content| read_from(std::io::Cursor::new(content.to_vec())).events)
            .collect();
        let reference = select_latest(all);
        assert_eq!(combined.invalid, reference.invalid);
        assert_eq!(combined.limits.len(), reference.limits.len());
        for (id, data) in &reference.limits {
            assert_eq!(combined.limits[id].date, data.date, "{id}");
            assert_eq!(combined.limits[id].limits, data.limits, "{id}");
        }
        let _ = std::fs::remove_dir_all(folder);
    }

    // I6
    #[test]
    fn files_that_are_no_longer_recent_are_forgotten() {
        let folder = dir("evict");
        let paths: Vec<PathBuf> = (0..3)
            .map(|index| {
                let path = folder.join(format!("rollout-{index}.jsonl"));
                std::fs::write(
                    &path,
                    event("2026-11-20T10:00:00.000Z", "codex", 1.0) + "\n",
                )
                .unwrap();
                path
            })
            .collect();
        let mut cache = SessionCache::new();
        for path in &paths {
            cache.read(path).unwrap();
        }
        assert_eq!(cache.tracked_files(), 3);
        cache.keep_only(&paths[..1]);
        assert_eq!(cache.tracked_files(), 1);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_missing_file_is_an_error_and_an_empty_one_is_an_empty_summary() {
        let folder = dir("missing");
        let mut cache = SessionCache::new();
        assert!(cache.read(&folder.join("nope.jsonl")).is_err());
        let empty = folder.join("rollout-empty.jsonl");
        std::fs::write(&empty, b"").unwrap();
        let got = cache.read(&empty).unwrap();
        assert!(got.selection.limits.is_empty() && got.errors_total == 0);
        let _ = std::fs::remove_dir_all(folder);
    }
}
