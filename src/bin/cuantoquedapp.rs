//! Tray icon: menu bar (Mac) or system tray (Windows).
// On Windows (release build) no console window is opened.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use cuantoquedapp::{
    cli::{parse_options, usage_tray, use_cli_hint},
    i18n, settings,
    single_instance::{self, Acquire},
    tray,
};

fn main() {
    // Only for the messages below; the tray works out its own language (see `tray::run`).
    let lang = || {
        let saved = settings::default_path()
            .map(|path| settings::load(&path))
            .unwrap_or_default();
        i18n::pick(i18n::env_override(), saved.language, i18n::system())
    };
    let options = match parse_options(std::env::args().skip(1)) {
        Ok(Some(options)) if !options.cli => options,
        Ok(Some(_)) => {
            let lang = lang();
            eprintln!("{}\n{}", use_cli_hint(lang), usage_tray(lang));
            std::process::exit(2);
        }
        Ok(None) => {
            println!("{}", usage_tray(lang()));
            return;
        }
        Err(error) => {
            let lang = lang();
            eprintln!("{}\n{}", error.text(lang), usage_tray(lang));
            std::process::exit(2);
        }
    };
    // Held until the process ends: the OS releases the lock on exit or crash.
    let _instance = match single_instance::acquire_default() {
        Ok(Acquire::Acquired(guard)) => Some(guard),
        // Another copy already shows the icon: exit silently, without a second one.
        Ok(Acquire::AlreadyRunning) => return,
        // Best effort: a lock problem must not stop the app from starting.
        Err(error) => {
            eprintln!(
                "Could not check whether another copy is running ({:?})",
                error.kind()
            );
            None
        }
    };
    // Live data only with the default folder; `--folder` means "read exactly these files".
    tray::run(options.folder, !options.custom_folder);
}
