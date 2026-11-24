//! Manual view of the limits. Takes an optional folder; never prints conversations.
use chrono::{Local, Utc};
use cuantoquedapp::{
    monitor::{default_folder, query},
    presentation::format_report,
};

fn main() {
    let folder = match std::env::args_os().nth(1).map(std::path::PathBuf::from) {
        Some(path) => path,
        None => match default_folder() {
            Ok(path) => path,
            Err(error) => {
                eprintln!("{error}");
                return;
            }
        },
    };
    println!(
        "{}",
        format_report(
            &query(&folder, Local::now().date_naive()),
            Utc::now(),
            &Local,
            cuantoquedapp::i18n::system()
        )
    );
}
