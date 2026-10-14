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

/// Write relative to `cwd` (or to an absolute path), creating parent directories.
pub fn write(args: &WriteArgs, cwd: &Path) -> io::Result<()> {
    let path = cwd.join(&args.path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, &args.content)
}
