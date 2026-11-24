//! Line reading with a memory limit: a line longer than the limit is skipped to its end
//! instead of being kept, so a damaged or hostile file cannot make the app allocate without bound.
use std::io::{self, BufRead};

/// Longest line ever kept (real Codex lines are about 1 MiB).
pub const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum LineRead {
    /// End of input; nothing was read.
    Eof,
    /// A line is in the buffer (with its `\n`, if it had one).
    Complete,
    /// The line exceeded the limit: it was skipped and the buffer is empty. `bytes` is how many
    /// bytes it had, and `terminated` whether it ended with `\n` (otherwise the input ended).
    TooLong { bytes: usize, terminated: bool },
}

/// Reads one line into `buffer` (cleared first), holding at most `max` bytes.
pub fn read_line_bounded<R: BufRead>(
    reader: &mut R,
    buffer: &mut Vec<u8>,
    max: usize,
) -> io::Result<LineRead> {
    buffer.clear();
    let mut too_long = false;
    let mut read_any = false;
    let mut total = 0usize;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(match (read_any, too_long) {
                (false, _) => LineRead::Eof,
                (true, true) => LineRead::TooLong {
                    bytes: total,
                    terminated: false,
                },
                (true, false) => LineRead::Complete,
            });
        }
        read_any = true;
        let (chunk, finished) = match available.iter().position(|byte| *byte == b'\n') {
            Some(position) => (&available[..=position], true),
            None => (available, false),
        };
        if !too_long {
            if buffer.len() + chunk.len() > max {
                too_long = true;
                buffer.clear();
            } else {
                buffer.extend_from_slice(chunk);
            }
        }
        let length = chunk.len();
        total += length;
        reader.consume(length);
        if finished {
            return Ok(if too_long {
                LineRead::TooLong {
                    bytes: total,
                    terminated: true,
                }
            } else {
                LineRead::Complete
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn lines(input: &[u8], max: usize) -> Vec<(LineRead, Vec<u8>)> {
        let mut reader = Cursor::new(input.to_vec());
        let mut buffer = Vec::new();
        let mut out = Vec::new();
        loop {
            let kind = read_line_bounded(&mut reader, &mut buffer, max).unwrap();
            if kind == LineRead::Eof {
                return out;
            }
            out.push((kind, buffer.clone()));
        }
    }

    // A4
    #[test]
    fn short_lines_are_returned_with_their_newline() {
        let got = lines(b"one\ntwo\nthree", 100);
        assert_eq!(got.len(), 3);
        assert_eq!(got[0], (LineRead::Complete, b"one\n".to_vec()));
        assert_eq!(
            got[2],
            (LineRead::Complete, b"three".to_vec()),
            "last line without newline"
        );
    }

    #[test]
    fn a_too_long_line_is_skipped_and_the_next_ones_are_read() {
        let mut input = vec![b'x'; 1000];
        input.extend_from_slice(b"\nok\n");
        let got = lines(&input, 100);
        assert_eq!(
            got[0],
            (
                LineRead::TooLong {
                    bytes: 1001,
                    terminated: true
                },
                Vec::new()
            )
        );
        assert_eq!(got[1], (LineRead::Complete, b"ok\n".to_vec()));
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn the_limit_is_inclusive_and_an_endless_line_is_reported_once() {
        assert_eq!(lines(b"abcd\n", 5)[0].0, LineRead::Complete);
        assert!(matches!(
            lines(b"abcde\n", 5)[0].0,
            LineRead::TooLong { .. }
        ));
        let got = lines(&vec![b'y'; 10_000], 100);
        assert_eq!(
            got,
            vec![(
                LineRead::TooLong {
                    bytes: 10_000,
                    terminated: false
                },
                Vec::new()
            )]
        );
        assert!(lines(b"", 10).is_empty());
    }

    #[test]
    fn memory_stays_bounded_while_skipping() {
        // A reader that would produce gigabytes if it were collected: only the limit is held.
        struct Endless(usize);
        impl io::Read for Endless {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                if self.0 == 0 {
                    return Ok(0);
                }
                let n = buf.len().min(self.0);
                buf[..n].fill(b'z');
                self.0 -= n;
                Ok(n)
            }
        }
        let mut reader = io::BufReader::with_capacity(8192, Endless(200 * 1024 * 1024));
        let mut buffer = Vec::new();
        let result = read_line_bounded(&mut reader, &mut buffer, 1024).unwrap();
        assert!(matches!(
            result,
            LineRead::TooLong {
                terminated: false,
                ..
            }
        ));
        assert!(buffer.capacity() < 64 * 1024, "{}", buffer.capacity());
    }
}
