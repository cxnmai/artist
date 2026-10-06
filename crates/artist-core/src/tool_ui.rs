//! Frontend-only tool I/O presentation, excluded from model replay and token accounting.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUi {
    /// Compact summary. Frontends may interpret ANSI SGR colors/styles.
    pub short: String,
    /// Expanded detail with optional ANSI styling; absent means raw I/O fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long: Option<String>,
}
