//! Command-line arguments, shared by the tray icon and the CLI.
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use rust_i18n::t;

use crate::{i18n::Lang, monitor};

pub fn usage_tray(lang: Lang) -> String {
    t!("cli.usage_tray", locale = lang.code()).into()
}

pub fn usage_cli(lang: Lang) -> String {
    t!("cli.usage_cli", locale = lang.code()).into()
}

/// The tray app was started with CLI-only arguments.
pub fn use_cli_hint(lang: Lang) -> String {
    t!("cli.use_cli", locale = lang.code()).into()
}

#[derive(Debug)]
pub struct Options {
    pub cli: bool,
    pub folder: PathBuf,
    /// `--folder` was given: the user wants exactly those files, so no live data is mixed in.
    pub custom_folder: bool,
    /// `--live` (CLI only): also ask Codex and Claude live, like the icon does.
    pub live: bool,
    pub now: Option<DateTime<Utc>>,
}

/// Why the arguments were refused; `text` says it in the user's language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    MissingFolder,
    MissingNow,
    BadNow,
    /// Unknown, repeated, or not allowed here.
    Unknown(String),
    LiveAndNow,
    NoHomeFolder,
}

impl CliError {
    pub fn text(&self, lang: Lang) -> String {
        let locale = lang.code();
        match self {
            CliError::MissingFolder => t!("cli.missing_folder", locale = locale),
            CliError::MissingNow => t!("cli.missing_now", locale = locale),
            CliError::BadNow => t!("cli.bad_now", locale = locale),
            CliError::Unknown(arg) => {
                t!(
                    "cli.unknown",
                    locale = locale,
                    arg = crate::presentation::clean(arg)
                )
            }
            CliError::LiveAndNow => t!("cli.live_and_now", locale = locale),
            CliError::NoHomeFolder => t!("cli.no_home", locale = locale),
        }
        .into()
    }
}

/// `Ok(None)` means "show the help".
pub fn parse_options(args: impl IntoIterator<Item = String>) -> Result<Option<Options>, CliError> {
    let mut args = args.into_iter().peekable();
    let cli = args.peek().is_some_and(|arg| arg == "codex");
    if cli {
        args.next();
    }
    let mut folder = None;
    let mut now = None;
    let mut live = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(None),
            "--folder" if folder.is_none() => {
                folder = Some(PathBuf::from(args.next().ok_or(CliError::MissingFolder)?));
            }
            "--now" if cli && now.is_none() => {
                let value = args.next().ok_or(CliError::MissingNow)?;
                now = Some(
                    DateTime::parse_from_rfc3339(&value)
                        .map_err(|_| CliError::BadNow)?
                        .with_timezone(&Utc),
                );
            }
            "--live" if cli && !live => live = true,
            _ => return Err(CliError::Unknown(arg)),
        }
    }
    if live && now.is_some() {
        return Err(CliError::LiveAndNow);
    }
    let custom_folder = folder.is_some();
    Ok(Some(Options {
        cli,
        custom_folder,
        live,
        folder: match folder {
            Some(path) => path,
            None => monitor::default_folder().map_err(|_| CliError::NoHomeFolder)?,
        },
        now,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Result<Option<Options>, CliError> {
        parse_options(values.iter().map(|s| s.to_string()))
    }

    #[test]
    fn cli_with_injected_time_and_folder() {
        let options = args(&[
            "codex",
            "--folder",
            "/tmp/fixtures",
            "--now",
            "2026-11-20T00:00:00Z",
        ])
        .unwrap()
        .unwrap();
        assert!(options.cli);
        assert_eq!(options.folder, PathBuf::from("/tmp/fixtures"));
        assert!(options.now.is_some());
    }

    #[test]
    fn live_flag_is_only_for_the_cli() {
        let options = args(&["codex", "--live"]).unwrap().unwrap();
        assert!(options.live);
        assert!(!args(&["codex"]).unwrap().unwrap().live);
    }

    #[test]
    fn incomplete_or_invalid_arguments_fail() {
        for case in [
            vec!["codex", "--folder"],
            vec!["codex", "--now", "yesterday"],
            vec!["--now", "2026-11-20T00:00:00Z"],
            vec!["unknown"],
            vec!["--live"],
            vec!["codex", "--live", "--live"],
            vec!["codex", "--live", "--now", "2026-11-20T00:00:00Z"],
        ] {
            assert!(args(&case).is_err(), "{case:?}");
        }
        assert!(args(&["--help"]).unwrap().is_none());
    }

    #[test]
    fn the_old_spanish_flags_are_gone() {
        for flag in ["--carpeta", "--ahora", "--vivo"] {
            assert_eq!(
                args(&["codex", flag]).unwrap_err(),
                CliError::Unknown(flag.into())
            );
        }
    }

    #[test]
    fn errors_and_usage_speak_both_languages() {
        let error = CliError::Unknown("--x".into());
        assert_eq!(
            error.text(Lang::Es),
            "Argumento desconocido o repetido: --x"
        );
        assert_eq!(error.text(Lang::En), "Unknown or repeated argument: --x");
        assert!(usage_cli(Lang::Es).starts_with("Uso: cuantoquedapp-cli [--folder <ruta>]"));
        assert!(usage_cli(Lang::En).starts_with("Usage: cuantoquedapp-cli [--folder <path>]"));
    }
}
