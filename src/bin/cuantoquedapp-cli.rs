//! The same limits as the tray icon, printed in the terminal.
use chrono::{Local, Utc};
use cuantoquedapp::{
    cli::{parse_options, usage_cli},
    i18n, monitor, presentation, settings,
    sources::{Sources, Tools},
};

fn main() {
    // `CUANTOQUEDAPP_LANG` first, then the language chosen in the app's settings, then the system's.
    let saved = settings::default_path()
        .map(|path| settings::load(&path))
        .unwrap_or_default();
    let lang = i18n::pick(i18n::env_override(), saved.language, i18n::system());
    // `codex` is optional here: `cuantoquedapp-cli` is the same as `cuantoquedapp-cli codex`.
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "codex") {
        args.remove(0);
    }
    args.insert(0, "codex".into());

    let options = match parse_options(args) {
        Ok(Some(options)) => options,
        Ok(None) => {
            println!("{}", usage_cli(lang));
            return;
        }
        Err(error) => {
            eprintln!("{}\n{}", error.text(lang), usage_cli(lang));
            std::process::exit(2);
        }
    };
    let report = if options.live {
        let events = Sources::from_environment().poll(Utc::now(), &Local, false, Tools::ALL);
        monitor::query_with(&options.folder, events, Local::now().date_naive())
    } else {
        monitor::query(&options.folder, Local::now().date_naive())
    };
    let now = options.now.unwrap_or_else(Utc::now);
    println!(
        "{}",
        presentation::format_report(&report, now, &Local, lang)
    );
}
