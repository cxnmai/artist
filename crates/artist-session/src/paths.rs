use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

pub(crate) fn directory(explicit: Option<&Path>) -> Result<PathBuf, String> {
    let path = match explicit {
        Some(path) => path.to_owned(),
        None => dirs::data_local_dir()
            .ok_or("cannot determine local data directory")?
            .join("artist/sessions"),
    };
    if !path.is_absolute() {
        return Err("session directory must be absolute".into());
    }
    Ok(path)
}

pub(crate) fn lock_path(directory: &Path, id: &str) -> Result<PathBuf, String> {
    let parsed = uuid::Uuid::parse_str(id).map_err(|_| "invalid session ID")?;
    if parsed.to_string() != id {
        return Err("session ID must be a canonical UUID".into());
    }
    Ok(directory.join("locks").join(format!("{id}.lock")))
}

pub(crate) fn private_dir(path: &Path) -> Result<(), String> {
    let missing = path
        .ancestors()
        .take_while(|path| !path.exists())
        .collect::<Vec<_>>();
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(|e| e.to_string())?;
    restrict(path, true)?;
    for created in missing {
        sync_dir(created)?;
        if let Some(parent) = created.parent() {
            sync_dir(parent)?;
        }
    }
    Ok(())
}

pub(crate) fn restrict(path: &Path, directory: bool) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if metadata.file_type().is_symlink() || metadata.is_dir() != directory {
        return Err(format!("invalid session storage path: {}", path.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(crate) fn private_file(path: &Path, new: bool) -> Result<File, String> {
    if !new && path.exists() {
        restrict(path, false)?;
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    if new {
        options.create_new(true);
    } else {
        options.create(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|e| e.to_string())
}

pub(crate) fn writer_lock(path: &Path) -> Result<File, String> {
    let file = private_file(path, false)?;
    file.try_lock_exclusive()
        .map_err(|e| format!("session already has a writer (or lock failed): {e}"))?;
    Ok(file)
}

// Persist newly created directory entries as well as the engine's file contents.
#[cfg(unix)]
pub(crate) fn sync_dir(path: &Path) -> Result<(), String> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|e| e.to_string())
}
#[cfg(not(unix))]
pub(crate) fn sync_dir(_: &Path) -> Result<(), String> {
    Ok(())
}
