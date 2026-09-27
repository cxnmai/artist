//! Text-only read tool.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const MAX_LINES: usize = 2_000;
const MAX_BYTES: usize = 50 * 1024;

pub struct ReadArgs {
    pub path: PathBuf,
    /// Starting line, 1-indexed.
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

pub struct ReadResult {
    pub text: String,
    /// First unread line, if there is more content.
    pub next_offset: Option<usize>,
}

/// Read UTF-8 text relative to `cwd` (or from an absolute path).
pub fn read(args: &ReadArgs, cwd: &Path) -> io::Result<ReadResult> {
    let offset = args.offset.unwrap_or(1);
    if offset == 0 || args.limit == Some(0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "offset and limit must be positive",
        ));
    }

    let content = fs::read_to_string(cwd.join(&args.path))?;
    let lines: Vec<&str> = content.split('\n').collect();
    let start = offset - 1;
    if start >= lines.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "offset is beyond the end of the file",
        ));
    }

    let mut text = String::new();
    let mut next = start;
    let end = start.saturating_add(args.limit.unwrap_or(MAX_LINES).min(MAX_LINES));
    while next < end.min(lines.len()) {
        let line = lines[next];
        let separator = if next == start { "" } else { "\n" };
        if text.len() + separator.len() + line.len() > MAX_BYTES {
            if next == start {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "line exceeds the 50 KB output limit",
                ));
            }
            break;
        }
        text.push_str(separator);
        text.push_str(line);
        next += 1;
    }

    Ok(ReadResult {
        text,
        next_offset: (next < lines.len()).then_some(next + 1),
    })
}
