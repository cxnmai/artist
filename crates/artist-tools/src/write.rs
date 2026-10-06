//! Create or overwrite a file with the supplied content.

use serde::Deserialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
pub struct WriteArgs {
    pub path: PathBuf,
    pub content: String,
}

/// Capture the old contents for presentation without changing write/error semantics.
/// Missing files are additions; unreadable/non-UTF-8 files have no textual before-image.
pub fn write_with_snapshot(args: &WriteArgs, cwd: &Path) -> io::Result<Option<String>> {
    let before = match fs::read_to_string(cwd.join(&args.path)) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Some(String::new()),
        Err(_) => None,
    };
    write(args, cwd)?;
    Ok(before)
}

/// Write relative to `cwd` (or to an absolute path), creating parent directories.
pub fn write(args: &WriteArgs, cwd: &Path) -> io::Result<()> {
    let path = cwd.join(&args.path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, &args.content)
}
