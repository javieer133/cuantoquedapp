//! Full reading shared by the CLI and the tray icon. Never exposes session lines.
use std::{
    ffi::OsString,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use chrono::NaiveDate;
use rust_i18n::t;

use crate::{
    i18n::Lang,
    reading::{LimitsEvent, LineProblem},
    selection_state::{Selection, select_latest},
    session_cache::SessionCache,
    session_files,
};

#[derive(Debug, Clone)]
pub struct Report {
    pub selection: Selection,
    pub notices: Vec<Notice>,
}

/// Something worth telling the user next to the limits. Kept as a value: the interface puts
/// it into words, in the user's language, with `text`.
#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    /// The reading thread is gone.
    ReaderLost,
    LineFormat {
        file: PathBuf,
        line: usize,
        reason: LineProblem,
    },
    /// More format errors in `file` than the ones listed one by one.
    MoreErrors {
        file: PathBuf,
        count: usize,
    },
    FileUnreadable {
        file: PathBuf,
        kind: ErrorKind,
    },
    CodexNotFound,
    SearchFailed(ErrorKind),
    /// Limit events with an invalid date or values.
    InvalidEvents(usize),
}

impl Notice {
    pub fn text(&self, lang: Lang) -> String {
        let locale = lang.code();
        match self {
            Notice::ReaderLost => t!("notice.reader_lost", locale = locale).into(),
            Notice::LineFormat { file, line, reason } => t!(
                "notice.line_format",
                locale = locale,
                file = file.display(),
                line = line,
                reason = reason.text(lang)
            )
            .into(),
            Notice::MoreErrors { file, count } => t!(
                "notice.more_errors",
                locale = locale,
                file = file.display(),
                count = count
            )
            .into(),
            Notice::FileUnreadable { file, kind } => t!(
                "notice.file_unreadable",
                locale = locale,
                file = file.display(),
                kind = format!("{kind:?}")
            )
            .into(),
            Notice::CodexNotFound => t!("notice.codex_not_found", locale = locale).into(),
            Notice::SearchFailed(kind) => t!(
                "notice.search_failed",
                locale = locale,
                kind = format!("{kind:?}")
            )
            .into(),
            Notice::InvalidEvents(count) => {
                t!("notice.invalid_events", locale = locale, count = count).into()
            }
        }
    }
}

/// Report shown when the reading thread is gone: no data and one clear notice.
/// Never keep showing the last values: they would silently go stale.
pub fn reader_lost_report() -> Report {
    Report {
        selection: Selection::default(),
        notices: vec![Notice::ReaderLost],
    }
}

/// Neither `CODEX_HOME`, `HOME` nor `USERPROFILE` is set: the user must pass `--folder`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoHomeFolder;

/// English, for logs and the examples; the CLI says it in the user's language (`CliError`).
impl std::fmt::Display for NoHomeFolder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("no home folder found (set HOME or CODEX_HOME)")
    }
}

/// Codex sessions folder: `$CODEX_HOME/sessions`, or `~/.codex/sessions`
/// (`HOME` on Mac/Linux, `USERPROFILE` on Windows).
pub fn default_folder() -> Result<PathBuf, NoHomeFolder> {
    folder_from(|name| std::env::var_os(name))
}

/// Core with the environment injected, to test it without touching real variables.
fn folder_from(environment: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf, NoHomeFolder> {
    let non_empty = |name: &str| environment(name).filter(|value| !value.is_empty());
    if let Some(codex_home) = non_empty("CODEX_HOME") {
        return Ok(PathBuf::from(codex_home).join("sessions"));
    }
    non_empty("HOME")
        .or_else(|| non_empty("USERPROFILE"))
        .map(|personal| PathBuf::from(personal).join(".codex").join("sessions"))
        .ok_or(NoHomeFolder)
}

/// Session files only. `today` (local date) limits how far back the day folders are walked.
pub fn query(folder: &Path, today: NaiveDate) -> Report {
    query_with(folder, Vec::new(), today)
}

/// Session files plus `live` events (already stamped by the caller). Both feed the same
/// pool, so the freshest value of each limit wins. Reads every file from the start; the
/// long-lived reader uses `query_cached` instead.
pub fn query_with(folder: &Path, live: Vec<LimitsEvent>, today: NaiveDate) -> Report {
    query_cached(folder, live, &mut SessionCache::new(), today)
}

/// Same result as `query_with`, but each session file is read only from where the last call
/// stopped (see `session_cache`).
pub fn query_cached(
    folder: &Path,
    live: Vec<LimitsEvent>,
    cache: &mut SessionCache,
    today: NaiveDate,
) -> Report {
    let has_live = !live.is_empty();
    // Live events go first: on ties they win over the files, as they always did.
    let mut selection = select_latest(live);
    let mut notices = Vec::new();
    match session_files::find_recent_rollout_files(folder, 5, today) {
        Ok(session_files) => {
            cache.keep_only(&session_files);
            for file in session_files {
                match cache.read(&file) {
                    Ok(summary) => {
                        selection.merge(&summary.selection);
                        // Cap the icon's message even with thousands of corrupt lines.
                        for error in &summary.errors {
                            notices.push(Notice::LineFormat {
                                file: file.clone(),
                                line: error.line,
                                reason: error.reason.clone(),
                            });
                        }
                        if summary.errors_total > summary.errors.len() {
                            notices.push(Notice::MoreErrors {
                                file: file.clone(),
                                count: summary.errors_total - summary.errors.len(),
                            });
                        }
                    }
                    Err(error) => notices.push(Notice::FileUnreadable {
                        file: file.clone(),
                        kind: error.kind(),
                    }),
                }
            }
        }
        // Live data proves Codex is here even if it keeps no session files.
        Err(session_files::Error::FolderNotFound) if has_live => cache.keep_only(&[]),
        Err(session_files::Error::FolderNotFound) => {
            cache.keep_only(&[]);
            notices.push(Notice::CodexNotFound)
        }
        Err(session_files::Error::ReadError(error)) => {
            notices.push(Notice::SearchFailed(error.kind()))
        }
    }
    if selection.invalid > 0 {
        notices.push(Notice::InvalidEvents(selection.invalid));
    }
    Report { selection, notices }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn environment(pairs: &[(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
        let pairs = pairs.to_vec();
        move |name| {
            pairs
                .iter()
                .find(|(clave, _)| *clave == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    #[test]
    fn codex_home_takes_priority() {
        let r = folder_from(environment(&[
            ("CODEX_HOME", "/datos/codex"),
            ("HOME", "/Users/x"),
        ]));
        assert_eq!(r.unwrap(), PathBuf::from("/datos/codex/sessions"));
    }

    #[test]
    fn without_codex_home_uses_home() {
        let r = folder_from(environment(&[("HOME", "/Users/x")]));
        assert_eq!(r.unwrap(), PathBuf::from("/Users/x/.codex/sessions"));
    }

    #[test]
    fn without_home_uses_windows_userprofile() {
        let r = folder_from(environment(&[("USERPROFILE", "C:/Users/x")]));
        assert_eq!(r.unwrap(), PathBuf::from("C:/Users/x/.codex/sessions"));
    }

    #[test]
    fn empty_values_are_ignored() {
        let r = folder_from(environment(&[
            ("CODEX_HOME", ""),
            ("HOME", ""),
            ("USERPROFILE", "/u"),
        ]));
        assert_eq!(r.unwrap(), PathBuf::from("/u/.codex/sessions"));
    }

    #[test]
    fn no_variable_is_a_clear_error() {
        let r = folder_from(environment(&[]));
        assert_eq!(r, Err(NoHomeFolder));
    }
}
