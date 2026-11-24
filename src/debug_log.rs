//! Opt-in diagnostics for problems that cannot be seen from a GUI app (no console on Windows):
//! when the variable `CUANTOQUEDAPP_DEBUG` is set, lines go to `debug.log` next to the
//! instance lock. Only event names, sizes and geometry: never data from the accounts.
//! The log is private (0600) and never written through a link.
use std::io::Write;

pub fn debug_log(message: &str) {
    if std::env::var_os("CUANTOQUEDAPP_DEBUG").is_none() {
        return;
    }
    let Some(folder) = crate::single_instance::default_lock_path()
        .and_then(|lock| lock.parent().map(std::path::Path::to_path_buf))
    else {
        return;
    };
    let path = folder.join("debug.log");
    if crate::atomic_write::private_folder(&folder).is_err()
        || std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink())
    {
        return;
    }
    if let Ok(mut file) = crate::atomic_write::private_file_options()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(
            file,
            "{} {message}",
            chrono::Utc::now().format("%H:%M:%S%.3f")
        );
    }
}
