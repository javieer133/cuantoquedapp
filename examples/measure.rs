//! Times the reading of the session files on real data and checks that the incremental
//! reading gives the same as a full one. Prints only durations, counts and sizes.
//! Usage: `cargo run --release --example measure` (optionally `-- /other/folder`).
use std::time::Instant;

use cuantoquedapp::{
    monitor::{Report, default_folder, query_cached, query_with},
    session_cache::SessionCache,
    session_files::find_recent_rollout_files,
};

fn same(left: &Report, right: &Report) -> bool {
    left.notices == right.notices
        && left.selection.invalid == right.selection.invalid
        && left.selection.limits.len() == right.selection.limits.len()
        && left.selection.limits.iter().all(|(id, data)| {
            right
                .selection
                .limits
                .get(id)
                .is_some_and(|other| other.date == data.date && other.limits == data.limits)
        })
}

fn main() {
    let today = chrono::Local::now().date_naive();
    let folder = match std::env::args_os().nth(1) {
        Some(path) => std::path::PathBuf::from(path),
        None => match default_folder() {
            Ok(path) => path,
            Err(error) => {
                eprintln!("{error}");
                return;
            }
        },
    };

    if let Ok(files) = find_recent_rollout_files(&folder, 5, today) {
        let sizes: Vec<u64> = files
            .iter()
            .filter_map(|f| std::fs::metadata(f).ok())
            .map(|m| m.len())
            .collect();
        let total: u64 = sizes.iter().sum();
        println!(
            "Files read per query: {} ({:.1} MB total)",
            sizes.len(),
            total as f64 / 1e6
        );
    }

    let mut full = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        let report = query_with(&folder, Vec::new(), today);
        full.push(start.elapsed().as_micros());
        std::hint::black_box(&report);
    }
    println!("full reading, 5 runs (us): {full:?}");

    let mut cache = SessionCache::new();
    let mut steady = Vec::new();
    let start = Instant::now();
    let first = query_cached(&folder, Vec::new(), &mut cache, today);
    println!(
        "incremental, first reading (us): {}",
        start.elapsed().as_micros()
    );
    for _ in 0..5 {
        let start = Instant::now();
        let report = query_cached(&folder, Vec::new(), &mut cache, today);
        steady.push(start.elapsed().as_micros());
        std::hint::black_box(&report);
    }
    println!("incremental, steady state, 5 runs (us): {steady:?}");
    let reference = query_with(&folder, Vec::new(), today);
    println!(
        "same result as the full reading: {} (first) / {} (after the steady reads)",
        same(&first, &reference),
        same(
            &query_cached(&folder, Vec::new(), &mut cache, today),
            &reference
        )
    );
    println!(
        "content bytes read by the cache so far: {}",
        cache.bytes_read()
    );
}
