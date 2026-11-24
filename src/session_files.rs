//! Finds the most recent Codex session files.
//!
//! Input: sessions folder (a parameter, never hardcoded), how many files to return and today's
//! date. Output: paths of `rollout-*.jsonl`, from newest to oldest by modification time (not by
//! the YYYY/MM/DD folder, which is the start date).
//!
//! Codex files sessions under `YYYY/MM/DD` (the day the session started). Day folders older
//! than `RECENT_DAYS` are not walked, so the cost stays the same after years of history. Limits
//! last a week at most; the margin covers old sessions resumed later. Folders whose names are
//! not dates are walked as before.
//!
//! An unreadable file or subfolder is skipped; only a missing or
//! unreadable root folder is an error. File contents are never opened.

use std::{
    fs::read_dir,
    path::{Path, PathBuf},
    time::SystemTime,
};

use chrono::{Datelike, Days, NaiveDate};

/// Day folders this many days before today or older are skipped.
pub const RECENT_DAYS: u64 = 30;

/// How deep the walk goes below the sessions folder (`YYYY/MM/DD` is 3; the rest is margin).
pub const MAX_DEPTH: usize = 8;
/// Folder entries looked at per walk, at most: a planted tree of millions of entries costs
/// this much and no more (a real one has a few hundred).
pub const MAX_ENTRIES: usize = 50_000;

/// Caps of one walk; `find_recent_rollout_files` uses the constants above.
#[derive(Debug, Clone, Copy)]
struct Caps {
    depth: usize,
    entries: usize,
}

/// State of one walk: what is left of the caps and the newest files found so far.
struct Walk {
    since: NaiveDate,
    count: usize,
    max_depth: usize,
    entries_left: usize,
    collected: Vec<(SystemTime, PathBuf)>,
}

impl Walk {
    fn add(&mut self, date: SystemTime, path: PathBuf) {
        self.collected.push((date, path));
        // Only the newest `count` are wanted: never hold many more than that.
        if self.collected.len() >= self.count.max(1) * 4 {
            self.keep_newest();
        }
    }

    /// Newest first; on equal dates, by path so the order is stable.
    fn keep_newest(&mut self) {
        self.collected
            .sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        self.collected.truncate(self.count);
    }
}

#[derive(Debug)]
pub enum Error {
    FolderNotFound,
    ReadError(std::io::Error),
}

// Lets `?` convert an `io::Error` into our `Error`.
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::ReadError(error)
    }
}

/// Returns up to `count` `rollout-*.jsonl` files under `path`, from the most recently modified
/// to the oldest, skipping day folders started `RECENT_DAYS` or more days before `today`.
pub fn find_recent_rollout_files(
    path: &Path,
    count: usize,
    today: NaiveDate,
) -> Result<Vec<PathBuf>, Error> {
    let caps = Caps {
        depth: MAX_DEPTH,
        entries: MAX_ENTRIES,
    };
    find_with_caps(path, count, today, caps)
}

fn find_with_caps(
    path: &Path,
    count: usize,
    today: NaiveDate,
    caps: Caps,
) -> Result<Vec<PathBuf>, Error> {
    if !path.is_dir() {
        return Err(Error::FolderNotFound);
    }

    let mut walk = Walk {
        since: today
            .checked_sub_days(Days::new(RECENT_DAYS))
            .unwrap_or(NaiveDate::MIN),
        count,
        max_depth: caps.depth,
        entries_left: caps.entries,
        collected: Vec::new(),
    };
    collect_files(path, Level::Root, 0, &mut walk)?;
    walk.keep_newest();

    Ok(walk.collected.into_iter().map(|(_, path)| path).collect())
}

/// Where a folder sits in the `YYYY/MM/DD` layout, as far as its names say.
#[derive(Debug, Clone, Copy)]
enum Level {
    /// The sessions folder itself.
    Root,
    Year(i32),
    Month(i32, u32),
    /// Inside a day folder, or a folder whose name is not part of the date layout.
    Other,
}

/// `Some(level)` to walk a subfolder named `name` found at `level`, `None` to skip it because
/// all of it is older than `since`. A name that does not fit the layout is walked as `Other`.
fn subfolder(level: Level, name: &str, since: NaiveDate) -> Option<Level> {
    let number = |digits: usize| {
        (name.len() == digits && name.bytes().all(|b| b.is_ascii_digit()))
            .then(|| name.parse::<u32>().ok())
            .flatten()
    };
    match level {
        Level::Root => match number(4) {
            Some(year) => {
                let year = year as i32;
                (year >= since.year()).then_some(Level::Year(year))
            }
            None => Some(Level::Other),
        },
        Level::Year(year) => match number(2).filter(|month| (1..=12).contains(month)) {
            Some(month) => ((year, month) >= (since.year(), since.month()))
                .then_some(Level::Month(year, month)),
            None => Some(Level::Other),
        },
        Level::Month(year, month) => {
            match number(2).and_then(|day| NaiveDate::from_ymd_opt(year, month, day)) {
                Some(date) => (date >= since).then_some(Level::Other),
                None => Some(Level::Other),
            }
        }
        Level::Other => Some(Level::Other),
    }
}

/// Walks `folder` recursively (skipping old day folders) and adds the valid files, until the
/// walk's caps run out.
fn collect_files(
    folder: &Path,
    level: Level,
    depth: usize,
    walk: &mut Walk,
) -> Result<(), std::io::Error> {
    for dir_entry in read_dir(folder)? {
        if walk.entries_left == 0 {
            break;
        }
        walk.entries_left -= 1;
        // Unreadable entry: skipped.
        let Ok(dir_entry) = dir_entry else { continue };
        // `file_type` does not follow symlinks: avoids infinite loops.
        let Ok(kind) = dir_entry.file_type() else {
            continue;
        };

        if kind.is_dir() {
            if depth >= walk.max_depth {
                continue;
            }
            let name = dir_entry.file_name();
            let next = match name.to_str() {
                Some(name) => subfolder(level, name, walk.since),
                None => Some(Level::Other),
            };
            if let Some(next) = next {
                // Unreadable subfolder: skipped, it does not abort everything.
                let _ = collect_files(&dir_entry.path(), next, depth + 1, walk);
            }
        } else if kind.is_file() && is_rollout(&dir_entry.file_name()) {
            // No readable modification time: skipped.
            let Ok(date) = dir_entry.metadata().and_then(|m| m.modified()) else {
                continue;
            };
            walk.add(date, dir_entry.path());
        }
    }
    Ok(())
}

/// `rollout-*.jsonl`
fn is_rollout(name: &std::ffi::OsStr) -> bool {
    match name.to_str() {
        Some(n) => n.starts_with("rollout-") && n.ends_with(".jsonl"),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::Duration};

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 21).unwrap()
    }

    /// Empty temporary folder, unique per test (never `~/.codex`).
    fn temp_folder(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("cuantoquedapp-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    /// Creates a file (and its folders) with a fixed modification time.
    fn create(root: &Path, relative: &str, seconds: u64) -> PathBuf {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let file = fs::File::create(&path).unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
            .unwrap();
        path
    }

    fn names(paths: &[PathBuf]) -> Vec<String> {
        paths
            .iter()
            .map(|r| r.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    // SEC-03: a planted tree costs the caps and no more.
    #[test]
    fn the_walk_stops_at_its_depth_and_entry_caps() {
        let root = temp_folder("caps");
        create(&root, "a/rollout-shallow.jsonl", 100);
        create(&root, "a/b/c/d/rollout-deep.jsonl", 200);
        let shallow = Caps {
            depth: 2,
            entries: 1000,
        };
        assert_eq!(
            names(&find_with_caps(&root, 5, today(), shallow).unwrap()),
            ["rollout-shallow.jsonl"]
        );
        for n in 0..20 {
            create(&root, &format!("flat/rollout-{n:02}.jsonl"), 300 + n);
        }
        let few = Caps {
            depth: 8,
            entries: 5,
        };
        assert!(find_with_caps(&root, 50, today(), few).unwrap().len() <= 5);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn only_the_newest_are_kept_while_walking() {
        let root = temp_folder("newest");
        for n in 0..40 {
            create(&root, &format!("rollout-{n:02}.jsonl"), 1000 + n);
        }
        let mut walk = Walk {
            since: NaiveDate::MIN,
            count: 3,
            max_depth: MAX_DEPTH,
            entries_left: MAX_ENTRIES,
            collected: Vec::new(),
        };
        collect_files(&root, Level::Root, 0, &mut walk).unwrap();
        assert!(walk.collected.len() < 12, "never many more than `count`");
        walk.keep_newest();
        assert_eq!(
            names(
                &walk
                    .collected
                    .into_iter()
                    .map(|(_, p)| p)
                    .collect::<Vec<_>>()
            ),
            ["rollout-39.jsonl", "rollout-38.jsonl", "rollout-37.jsonl"]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sorts_newest_to_oldest() {
        let root = temp_folder("orden");
        create(&root, "2026/11/01/rollout-a.jsonl", 1000);
        create(&root, "2026/11/02/rollout-b.jsonl", 3000);
        create(&root, "2026/11/03/rollout-c.jsonl", 2000);

        let r = find_recent_rollout_files(&root, 10, today()).unwrap();
        assert_eq!(
            names(&r),
            ["rollout-b.jsonl", "rollout-c.jsonl", "rollout-a.jsonl"]
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn respects_the_limit() {
        let root = temp_folder("limite");
        create(&root, "rollout-a.jsonl", 1000);
        create(&root, "rollout-b.jsonl", 3000);
        create(&root, "rollout-c.jsonl", 2000);

        let r = find_recent_rollout_files(&root, 2, today()).unwrap();
        assert_eq!(names(&r), ["rollout-b.jsonl", "rollout-c.jsonl"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ignores_unrelated_files() {
        let root = temp_folder("ajenos");
        create(&root, "rollout-ok.jsonl", 1000);
        create(&root, "notas.txt", 9000);
        create(&root, "rollout-x.txt", 9000);
        create(&root, "otro-rollout.jsonl", 9000);

        let r = find_recent_rollout_files(&root, 10, today()).unwrap();
        assert_eq!(names(&r), ["rollout-ok.jsonl"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_folder_is_an_error() {
        let path = std::env::temp_dir().join("cuantoquedapp-no-existe-xyz");
        let r = find_recent_rollout_files(&path, 5, today());
        assert!(matches!(r, Err(Error::FolderNotFound)));
    }

    #[test]
    fn empty_folder_returns_empty_list() {
        let root = temp_folder("vacia");
        let r = find_recent_rollout_files(&root, 5, today()).unwrap();
        assert!(r.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn old_folder_but_modified_later_comes_first() {
        let root = temp_folder("antigua");
        // Session started on the 1st but used until recently.
        create(&root, "2026/11/01/rollout-vieja.jsonl", 9000);
        create(&root, "2026/11/20/rollout-nueva.jsonl", 5000);

        let r = find_recent_rollout_files(&root, 10, today()).unwrap();
        assert_eq!(names(&r), ["rollout-vieja.jsonl", "rollout-nueva.jsonl"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn day_folders_older_than_the_limit_are_not_walked() {
        let root = temp_folder("tope");
        // 21-nov minus 30 days = 22-oct: that day is in, the 21st is out.
        create(&root, "2026/10/21/rollout-fuera.jsonl", 9000);
        create(&root, "2026/10/22/rollout-borde.jsonl", 1000);
        create(&root, "2026/11/01/rollout-dentro.jsonl", 2000);
        create(&root, "2025/12/31/rollout-anio-pasado.jsonl", 9500);
        create(&root, "2026/09/30/rollout-mes-viejo.jsonl", 9600);

        let r = find_recent_rollout_files(&root, 10, today()).unwrap();
        assert_eq!(names(&r), ["rollout-dentro.jsonl", "rollout-borde.jsonl"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_limit_crosses_month_and_year_boundaries() {
        let root = temp_folder("cruce");
        create(&root, "2025/12/05/rollout-fuera.jsonl", 3000);
        create(&root, "2025/12/20/rollout-diciembre.jsonl", 1000);
        create(&root, "2026/01/03/rollout-enero.jsonl", 2000);
        let january = NaiveDate::from_ymd_opt(2026, 1, 10).unwrap();

        let r = find_recent_rollout_files(&root, 10, january).unwrap();
        assert_eq!(
            names(&r),
            ["rollout-enero.jsonl", "rollout-diciembre.jsonl"]
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn folders_that_are_not_dates_are_still_walked() {
        let root = temp_folder("nombres");
        create(&root, "archivadas/rollout-a.jsonl", 1000);
        create(&root, "2026/extra/rollout-b.jsonl", 2000);
        create(&root, "2026/11/hoy/rollout-c.jsonl", 3000);
        create(&root, "2026/11/99/rollout-d.jsonl", 4000);
        create(&root, "1999/01/01/rollout-viejo.jsonl", 5000);

        let r = find_recent_rollout_files(&root, 10, today()).unwrap();
        assert_eq!(
            names(&r),
            [
                "rollout-d.jsonl",
                "rollout-c.jsonl",
                "rollout-b.jsonl",
                "rollout-a.jsonl"
            ]
        );
        let _ = fs::remove_dir_all(&root);
    }
}
