//! Exact text replacements in a single file.

use serde::Deserialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Replacement {
    pub old_text: String,
    pub new_text: String,
}

#[derive(Deserialize)]
pub struct EditArgs {
    pub path: PathBuf,
    pub edits: Vec<Replacement>,
}

pub struct EditResult {
    pub replacements: usize,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

/// Validate every match against the original file before writing anything.
pub fn edit(args: &EditArgs, cwd: &Path) -> io::Result<EditResult> {
    if args.edits.is_empty() {
        return Err(invalid("edits must not be empty"));
    }

    let path = cwd.join(&args.path);
    let original = fs::read_to_string(&path)?;
    let mut matches = Vec::with_capacity(args.edits.len());
    for (index, replacement) in args.edits.iter().enumerate() {
        if replacement.old_text.is_empty() {
            return Err(invalid(format!(
                "edits[{index}].old_text must not be empty"
            )));
        }
        let mut occurrences = original.match_indices(&replacement.old_text);
        let Some((start, _)) = occurrences.next() else {
            return Err(invalid(format!("edits[{index}].old_text was not found")));
        };
        if occurrences.next().is_some() {
            return Err(invalid(format!("edits[{index}].old_text is not unique")));
        }
        matches.push((start, start + replacement.old_text.len(), index));
    }

    matches.sort_by_key(|&(start, _, _)| start);
    for pair in matches.windows(2) {
        if pair[0].1 > pair[1].0 {
            return Err(invalid(format!(
                "edits[{}] and edits[{}] overlap",
                pair[0].2, pair[1].2
            )));
        }
    }

    let mut updated = String::with_capacity(original.len());
    let mut cursor = 0;
    for (start, end, index) in matches {
        updated.push_str(&original[cursor..start]);
        updated.push_str(&args.edits[index].new_text);
        cursor = end;
    }
    updated.push_str(&original[cursor..]);
    if updated == original {
        return Err(invalid("edits did not change the file"));
    }
    fs::write(path, updated)?;
    Ok(EditResult {
        replacements: args.edits.len(),
    })
}
