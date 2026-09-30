//! Runtime-selected model settings passed to Lua compactors and the builtin.

use crate::model::LuaModel;
use artist_core::compaction::CompactionSettings;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct SummaryOptions {
    #[serde(flatten)]
    pub settings: CompactionSettings,
    pub model_name: String,
    pub adapter_name: Option<String>,
    pub reasoning_level: Option<String>,
    pub thinking_format: Option<String>,
    #[serde(default)]
    pub requires_reasoning_content: bool,
}

impl SummaryOptions {
    pub fn for_model(model: &LuaModel<'_>, settings: CompactionSettings) -> Self {
        Self {
            settings,
            model_name: model.model_name.clone(),
            adapter_name: model.adapter_name.clone(),
            reasoning_level: model.reasoning_level.clone(),
            thinking_format: model.thinking_format.clone(),
            requires_reasoning_content: model.requires_reasoning_content,
        }
    }
}

/// Apply a standalone summary budget without changing the configured main request.
pub fn output_options(
    protocol: crate::registry::Protocol,
    options: &serde_json::Map<String, serde_json::Value>,
    model_name: &str,
    budget: Option<u64>,
) -> serde_json::Map<String, serde_json::Value> {
    use crate::registry::Protocol;
    let mut options = options.clone();
    if let Some(budget) = budget {
        let completion_limit = options.contains_key("max_completion_tokens")
            || ["gpt-5", "o1", "o3", "o4"]
                .iter()
                .any(|prefix| model_name.starts_with(prefix));
        options.remove("max_tokens");
        options.remove("max_completion_tokens");
        options.remove("max_output_tokens");
        // A standalone summarizer does not inherit automatic server compaction.
        options.remove("context_management");
        options.remove("compaction");
        options.remove("tool_choice");
        options.remove("parallel_tool_calls");
        options.remove("stream_options");
        options.remove("response_format");
        let key = match protocol {
            Protocol::ChatCompletions if completion_limit => "max_completion_tokens",
            Protocol::ChatCompletions => "max_tokens",
            Protocol::OpenAIResponses => "max_output_tokens",
            Protocol::AnthropicMessages => {
                options.remove("thinking");
                "max_tokens"
            }
        };
        options.insert(key.into(), serde_json::json!(budget));
    }
    options
}
