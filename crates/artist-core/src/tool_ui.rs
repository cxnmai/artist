//! Frontend-only tool I/O presentation, excluded from model replay and token accounting.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUi {
    /// Compact, plain-text summary of the tool operation.
    pub short: String,
    /// Expanded plain-text detail. If absent, frontends can inspect raw I/O instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long: Option<String>,
}
