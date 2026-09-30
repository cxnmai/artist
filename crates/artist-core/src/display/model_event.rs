//! Displayable model events, excluding native replay items.

use crate::event::ModelEvent;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DisplayModelEvent {
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total_tokens: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cached_input_tokens: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_write_input_tokens: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning_output_tokens: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cost_usd: Option<f64>,
    },
    Finished {
        reason: Option<String>,
    },
}

impl DisplayModelEvent {
    /// Strip native replay items when projecting internal events for a frontend.
    pub fn from_model(event: ModelEvent) -> Option<Self> {
        Some(match event {
            ModelEvent::ProviderData { .. } => return None,
            ModelEvent::TextDelta { text } => Self::TextDelta { text },
            ModelEvent::ThinkingDelta { text } => Self::ThinkingDelta { text },
            ModelEvent::ToolCallStart { id, name } => Self::ToolCallStart { id, name },
            ModelEvent::ToolCallArgumentsDelta { id, text } => {
                Self::ToolCallArgumentsDelta { id, text }
            }
            ModelEvent::ToolCallEnd { id } => Self::ToolCallEnd { id },
            ModelEvent::Usage {
                input_tokens,
                output_tokens,
                total_tokens,
                cached_input_tokens,
                cache_write_input_tokens,
                reasoning_output_tokens,
                cost_usd,
            } => Self::Usage {
                input_tokens,
                output_tokens,
                total_tokens,
                cached_input_tokens,
                cache_write_input_tokens,
                reasoning_output_tokens,
                cost_usd,
            },
            ModelEvent::Finished { reason } => Self::Finished { reason },
        })
    }
}
