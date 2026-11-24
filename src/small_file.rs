//! Reads the app's own small files (settings, the Claude capture, the statusline chain) with a
//! size cap: a huge or ever-growing file there is a broken or planted one, and is treated as
//! unreadable instead of being loaded whole into memory.
use std::{
    fs::File,
    io::{self, Read},
    path::Path,
};

/// Far above any real one (they are a few hundred bytes).
pub const MAX_BYTES: u64 = 64 * 1024;

/// The whole file, or an error if it cannot be read or is larger than `max` bytes.
pub fn read(path: &Path, max: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(max.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file larger than expected",
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_files_are_read_and_larger_ones_refused() {
        let path = std::env::temp_dir().join(format!("cuantoquedapp-small-{}", std::process::id()));
        std::fs::write(&path, b"12345").unwrap();
        assert_eq!(read(&path, 5).unwrap(), b"12345");
        assert_eq!(
            read(&path, 4).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        let _ = std::fs::remove_file(&path);
        assert!(read(&path, 5).is_err(), "missing");
    }
}
