//! Asks a real `codex app-server` for the live limits and prints them like the app does.
//! Usage: `cargo run --example live_report -- /path/to/codex`
//! Prints only limits fields (percentages, windows, reset times), never account data.
use std::{path::PathBuf, time::Instant};

use chrono::{Local, Utc};
use cuantoquedapp::{
    app_server::{AppServer, DEFAULT_TIMEOUT},
    live_codex::events_from_answer,
    monitor::Report,
    presentation::format_report,
    selection_state::select_latest,
};

fn main() {
    let Some(binary) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("Usage: live_report <path to the codex binary>");
        std::process::exit(2);
    };
    let started = Instant::now();
    let answer = match AppServer::start(&binary, DEFAULT_TIMEOUT)
        .and_then(|mut server| server.read_rate_limits(DEFAULT_TIMEOUT))
    {
        Ok(answer) => answer,
        Err(error) => {
            eprintln!("Could not read the live limits: {error}");
            std::process::exit(1);
        }
    };
    println!("(answer in {} ms)", started.elapsed().as_millis());
    let events = match events_from_answer(&answer, Utc::now()) {
        Ok(events) => events,
        Err(_) => {
            eprintln!("The answer from Codex does not have the expected shape");
            std::process::exit(1);
        }
    };
    let report = Report {
        selection: select_latest(events),
        notices: Vec::new(),
    };
    println!(
        "{}",
        format_report(&report, Utc::now(), &Local, cuantoquedapp::i18n::system())
    );
}
