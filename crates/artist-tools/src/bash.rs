//! Run one shell command in a fresh process.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};

pub struct BashArgs {
    pub command: String,
    /// No timeout when omitted.
    pub timeout: Option<Duration>,
}

pub struct BashResult {
    /// Combined stdout and stderr in observed arrival order.
    pub output: String,
    pub exit_code: Option<i32>,
    pub truncated: bool,
    pub full_output_path: Option<PathBuf>,
}

/// Spawn a non-interactive shell; `cd` and `export` do not survive this call.
pub async fn bash(args: &BashArgs, cwd: &Path) -> io::Result<BashResult> {
    bash_with_updates(args, cwd, |_| {}).await
}

/// Report bounded output snapshots while the command runs (at most every 100 ms).
pub async fn bash_with_updates(
    args: &BashArgs,
    cwd: &Path,
    mut on_update: impl FnMut(&str),
) -> io::Result<BashResult> {
    let shell = if Path::new("/bin/bash").exists() {
        "/bin/bash"
    } else {
        "bash"
    };
    let mut command = Command::new(shell);
    command
        .arg("-c")
        .arg(&args.command)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let child = command.spawn()?;
    if let Some(timeout) = args.timeout {
        tokio::time::timeout(timeout, collect(child, &mut on_update))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "bash command timed out"))?
    } else {
        collect(child, &mut on_update).await
    }
}

async fn collect(child: Child, on_update: &mut impl FnMut(&str)) -> io::Result<BashResult> {
    let mut child = child;
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let mut out_open = true;
    let mut err_open = true;
    let mut out_buf = [0; 8192];
    let mut err_buf = [0; 8192];
    let mut output = Output::new();
    let mut last_update = Instant::now() - Duration::from_millis(100);

    while out_open || err_open {
        let chunk = tokio::select! {
            read = stdout.read(&mut out_buf), if out_open => {
                let size = read?;
                out_open = size != 0;
                &out_buf[..size]
            }
            read = stderr.read(&mut err_buf), if err_open => {
                let size = read?;
                err_open = size != 0;
                &err_buf[..size]
            }
        };
        output.push(chunk).await?;
        if !chunk.is_empty() && last_update.elapsed() >= Duration::from_millis(100) {
            on_update(&output.display());
            last_update = Instant::now();
        }
    }
    let status = child.wait().await?;
    output.finish().await?;
    let result = output.display();
    on_update(&result);
    Ok(BashResult {
        output: result,
        exit_code: status.code(),
        truncated: output.truncated(),
        full_output_path: output.full_output_path,
    })
}

const MAX_BYTES: usize = 50 * 1024;
const MAX_LINES: usize = 2_000;
const TAIL_BYTES: usize = MAX_BYTES * 2;

struct Output {
    tail: Vec<u8>,
    pending: Vec<u8>,
    file: Option<File>,
    full_output_path: Option<PathBuf>,
    bytes: usize,
    newlines: usize,
    ends_with_newline: bool,
}

impl Output {
    fn new() -> Self {
        Self {
            tail: Vec::new(),
            pending: Vec::new(),
            file: None,
            full_output_path: None,
            bytes: 0,
            newlines: 0,
            ends_with_newline: false,
        }
    }

    fn truncated(&self) -> bool {
        self.bytes > MAX_BYTES || self.lines() > MAX_LINES
    }

    fn lines(&self) -> usize {
        self.newlines + usize::from(self.bytes > 0 && !self.ends_with_newline)
    }

    async fn push(&mut self, chunk: &[u8]) -> io::Result<()> {
        if chunk.is_empty() {
            return Ok(());
        }
        self.bytes += chunk.len();
        self.newlines += chunk.iter().filter(|&&byte| byte == b'\n').count();
        self.ends_with_newline = chunk.last() == Some(&b'\n');
        self.tail.extend_from_slice(chunk);
        if self.tail.len() > TAIL_BYTES {
            self.tail.drain(..self.tail.len() - TAIL_BYTES);
        }

        if self.file.is_none() {
            self.pending.extend_from_slice(chunk);
            if self.truncated() {
                let temp = tempfile::Builder::new().prefix("artist-bash-").tempfile()?;
                let (file, path) = temp.keep().map_err(|error| error.error)?;
                self.full_output_path = Some(path);
                self.file = Some(File::from_std(file));
                self.file.as_mut().unwrap().write_all(&self.pending).await?;
                self.pending.clear();
            }
        } else {
            self.file.as_mut().unwrap().write_all(chunk).await?;
        }
        Ok(())
    }

    async fn finish(&mut self) -> io::Result<()> {
        if let Some(file) = &mut self.file {
            file.flush().await?;
        }
        Ok(())
    }

    fn display(&self) -> String {
        let decoded = String::from_utf8_lossy(&self.tail);
        if !self.truncated() {
            return decoded.into_owned();
        }
        let mut start = decoded.len().saturating_sub(MAX_BYTES);
        while !decoded.is_char_boundary(start) {
            start += 1;
        }
        let mut result = &decoded[start..];
        // Prefer complete lines, except when the last line alone exceeds the limit.
        if start > 0
            && decoded.as_bytes()[start - 1] != b'\n'
            && let Some(newline) = result.find('\n')
            && newline + 1 < result.len()
        {
            result = &result[newline + 1..];
        }
        let boundary = MAX_LINES - usize::from(!result.ends_with('\n'));
        if let Some(start) = result.rmatch_indices('\n').nth(boundary) {
            result = &result[start.0 + 1..];
        }
        result.to_owned()
    }
}
