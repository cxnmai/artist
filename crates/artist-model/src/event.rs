//! Events consumed by the core and frontends, regardless of provider format.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelEvent {
    TextDelta {
        text: String,
    },
    ThinkingDelta {
        text: String,
    },
    ToolCallStart {
        id: String,
        name: String,
    },
    ToolCallArgumentsDelta {
        id: String,
        text: String,
    },
    ToolCallEnd {
        id: String,
    },
    Usage {
        input_tokens: u64,
        output_tokens: u64,
    },
    Finished {
        reason: Option<String>,
    },
}
