//! Data helpers for Lua-owned tool presentation; no labels, messages, or colors.

use similar::TextDiff;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

pub fn path(path: &Path, cwd: &Path) -> String {
    let root = cwd.canonicalize().unwrap_or_else(|_| normalize(cwd));
    let absolute = cwd.join(path);
    let absolute = absolute
        .canonicalize()
        .unwrap_or_else(|_| normalize(&absolute));
    absolute
        .strip_prefix(&root)
        .unwrap_or(&absolute)
        .display()
        .to_string()
}

fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            component => result.push(component.as_os_str()),
        }
    }
    result
}

/// None means no readable UTF-8 before-image; an empty diff means no changes.
pub fn diff(before: Option<&str>, after: &str, path: &str) -> Option<String> {
    let before = before?;
    if before == after {
        return Some(String::new());
    }
    let diff = TextDiff::configure()
        .timeout(Duration::from_millis(250))
        .diff_lines(before, after);
    Some(
        diff.unified_diff()
            .context_radius(3)
            .header(path, path)
            .to_string(),
    )
}
