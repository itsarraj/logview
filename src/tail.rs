use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// Polls one file for newly-appended bytes since the last read, handling a
/// trailing line that hasn't been terminated with `\n` yet by holding it
/// back until it is (or until the file is polled again and it still isn't
/// — at which point it's still held, correctly, since it's genuinely not a
/// complete line yet).
pub struct TailSource {
    pub name: String,
    file: File,
    pos: u64,
    leftover: Vec<u8>,
}

impl TailSource {
    pub fn open(path: &Path, name: impl Into<String>) -> io::Result<Self> {
        let file = File::open(path)?;
        Ok(Self {
            name: name.into(),
            file,
            pos: 0,
            leftover: Vec::new(),
        })
    }

    /// Seeks to within `cap_bytes` of the end and returns whatever complete
    /// lines are found there — bounds the initial read on a huge
    /// pre-existing file instead of loading it all. Stated explicitly
    /// rather than silently truncating: callers should surface `cap_bytes`
    /// to the user (the CLI's `--seed-bytes` flag, documented in --help).
    pub fn seed_from_near_end(&mut self, cap_bytes: u64) -> io::Result<Vec<String>> {
        let len = self.file.metadata()?.len();
        self.pos = len.saturating_sub(cap_bytes);
        self.poll()
    }

    /// Reads whatever is new since the last call and returns complete
    /// lines (`\n`- or `\r\n`-terminated). Bytes since the last newline are
    /// held in `leftover` and prefixed onto the next read.
    pub fn poll(&mut self) -> io::Result<Vec<String>> {
        self.file.seek(SeekFrom::Start(self.pos))?;
        let mut chunk = Vec::new();
        self.file.read_to_end(&mut chunk)?;
        if chunk.is_empty() {
            return Ok(Vec::new());
        }
        self.pos += chunk.len() as u64;
        self.leftover.extend_from_slice(&chunk);

        let mut lines = Vec::new();
        while let Some(idx) = self.leftover.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = self.leftover.drain(..=idx).collect();
            let without_newline = &line_bytes[..line_bytes.len() - 1];
            let text = String::from_utf8_lossy(without_newline);
            lines.push(text.trim_end_matches('\r').to_string());
        }
        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("logview-test-{}-{}", std::process::id(), name))
    }

    #[test]
    fn poll_returns_complete_lines_and_holds_partial_ones() {
        let path = temp_path("partial");
        std::fs::write(&path, b"first line\nsecond line\nno newline yet").unwrap();
        let mut source = TailSource::open(&path, "test").unwrap();

        let lines = source.poll().unwrap();
        assert_eq!(lines, vec!["first line", "second line"]);

        // Appending the rest of the partial line plus a terminator should
        // now surface it as one complete line, not split.
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(f, " — completed").unwrap();
        drop(f);

        let lines = source.poll().unwrap();
        assert_eq!(lines, vec!["no newline yet — completed"]);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn poll_with_nothing_new_returns_empty() {
        let path = temp_path("idle");
        std::fs::write(&path, b"one line\n").unwrap();
        let mut source = TailSource::open(&path, "test").unwrap();
        assert_eq!(source.poll().unwrap(), vec!["one line"]);
        assert_eq!(source.poll().unwrap(), Vec::<String>::new());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn crlf_line_endings_are_handled() {
        let path = temp_path("crlf");
        std::fs::write(&path, b"windows style\r\nanother\r\n").unwrap();
        let mut source = TailSource::open(&path, "test").unwrap();
        assert_eq!(source.poll().unwrap(), vec!["windows style", "another"]);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn seed_from_near_end_bounds_the_initial_read() {
        let path = temp_path("seed");
        let mut contents = String::new();
        for i in 0..1000 {
            contents.push_str(&format!("line {i}\n"));
        }
        std::fs::write(&path, contents.as_bytes()).unwrap();
        let mut source = TailSource::open(&path, "test").unwrap();

        // Cap small enough to guarantee we don't get all 1000 lines.
        let lines = source.seed_from_near_end(200).unwrap();
        assert!(lines.len() < 1000);
        assert!(!lines.is_empty());
        // The very last line must still be the file's actual last line.
        assert_eq!(lines.last().unwrap(), "line 999");

        std::fs::remove_file(&path).ok();
    }
}
