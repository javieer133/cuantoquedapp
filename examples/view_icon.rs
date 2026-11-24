//! Prints the tray icon as ASCII art. Usage: `cargo run --example view_icon -- 19` (also `?` or `!`).
use cuantoquedapp::tray_badge::{BadgeState, SIZE, render};

fn main() {
    let state = match std::env::args().nth(1).as_deref() {
        Some("?") | None => BadgeState::Unknown,
        Some("!") => BadgeState::Stale,
        Some(value) => match value.parse::<u8>() {
            Ok(percent) => BadgeState::Percent(percent),
            Err(_) => {
                eprintln!("Use a number 0-100, '?' or '!'");
                return;
            }
        },
    };
    let pixels = render(state);
    for y in 0..SIZE {
        let row: String = (0..SIZE)
            .map(|x| {
                let p = &pixels[(y * SIZE + x) * 4..(y * SIZE + x) * 4 + 4];
                match (p[3], p[0]) {
                    (0, _) => ' ',
                    (_, 255) => '#',
                    _ => '.',
                }
            })
            .collect();
        println!("{row}");
    }
}
