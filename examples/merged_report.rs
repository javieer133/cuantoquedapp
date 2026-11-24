//! Prints the report the tray icon would show: live Codex data merged with the session files.
//! Usage: `cargo run --example merged_report`
//! Prints only limits fields, never account or conversation data.
use chrono::{Local, Utc};
use cuantoquedapp::{
    claude_usage::ClaudeUsageSource,
    codex_binary::{Locations, find_codex_binary},
    live_source::LiveSource,
    monitor::{default_folder, query_with},
    presentation::format_report,
};

fn main() {
    let folder = match default_folder() {
        Ok(folder) => folder,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let binary = find_codex_binary(&Locations::from_environment());
    println!(
        "(Codex binary: {})",
        if binary.is_some() {
            "encontrado"
        } else {
            "no encontrado"
        }
    );
    let live = LiveSource::with_binary(binary).poll(Utc::now());
    let mut live = live;
    live.extend(ClaudeUsageSource::from_environment().poll(Utc::now(), &Local));
    println!("(live events: {})", live.len());
    println!(
        "{}",
        format_report(
            &query_with(&folder, live, Local::now().date_naive()),
            Utc::now(),
            &Local,
            cuantoquedapp::i18n::system()
        )
    );
}
