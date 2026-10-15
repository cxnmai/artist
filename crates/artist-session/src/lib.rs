//! Local durable sessions backed by the native embedded Turso engine.
//!
//! A directory contains one indexed `sessions.db` and private advisory lock files.
//! Turso's experimental multiprocess WAL is not enabled: operations acquire a
//! global OS lock and close all engine handles before releasing it. Models/tools
//! run without holding the database lock; each session retains its own writer lock.
//! Explicit directories must be absolute; defaults use the platform's local data
//! directory under `artist/sessions` (XDG data on Linux, Application Support on
//! macOS, LOCALAPPDATA on Windows). Unix directories/files are private from creation.
mod access;
mod blocking;
mod database;
mod lifecycle;
mod paths;
mod queries;
mod replay;
mod replay_state;
mod storage;
mod transcript;

pub use access::{list, metadata};
pub use transcript::{TranscriptItem, TranscriptPage, transcript};

use artist_core::{
    context::{ContextEntry, Conversation},
    journal::{Journal, SessionRecord},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
};
use storage::StoredRecord;
use tokio::sync::Mutex;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub provider: String,
    pub model: String,
    pub reasoning: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub cwd: PathBuf,
    /// Unix epoch milliseconds.
    pub started_at: i64,
    /// Unix epoch milliseconds.
    pub last_used_at: i64,
    pub settings: Settings,
}

struct Writer {
    directory: PathBuf,
    failed: bool,
}

pub struct Session {
    pub id: String,
    pub cwd: PathBuf,
    pub settings: Settings,
    /// Restored projection; Journal appends do not mutate this snapshot.
    /// The engine owns live conversation updates, except during `set_settings`.
    pub conversation: Conversation,
    writer: Mutex<Writer>,
    // In-flight blocking operations retain this lock even if their caller is dropped.
    // Never remove the file: its inode identity coordinates competing processes.
    _writer_lock: Arc<File>,
}
