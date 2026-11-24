//! Writes the panel page for the real current data to a file, to look at it in a browser.
//! Usage: `cargo run --example panel_html -- out.html`
use chrono::{Local, Utc};
use cuantoquedapp::{
    codex_binary::{Locations, find_codex_binary},
    live_source::LiveSource,
    monitor::{default_folder, query_with},
    panel_view::panel_view,
};

fn main() {
    let (Some(out), Ok(folder)) = (std::env::args_os().nth(1), default_folder()) else {
        eprintln!("Usage: panel_html <output file>");
        std::process::exit(2);
    };
    let live =
        LiveSource::with_binary(find_codex_binary(&Locations::from_environment())).poll(Utc::now());
    let view = panel_view(
        &query_with(&folder, live, Local::now().date_naive()),
        Utc::now(),
        &Local,
        cuantoquedapp::i18n::system(),
    );
    if std::fs::write(&out, &view.html).is_err() {
        eprintln!("Could not write the file");
        std::process::exit(1);
    }
    println!("estimated height: {} px", view.height);
}
