//! Model metadata known from Lua or a provider catalog. Unknown fields stay absent.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ModelMetadata {
    pub adapter: Option<String>,
    pub context_window: Option<u64>,
    pub max_output_tokens: Option<u64>,
    pub reasoning_levels: Option<Vec<String>>,
    pub reasoning_map: Option<BTreeMap<String, String>>,
    pub thinking_format: Option<String>,
    pub requires_reasoning_content: Option<bool>,
    pub default_reasoning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_levels: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_map: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_reasoning_content: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_reasoning: Option<String>,
}

impl ModelInfo {
    pub fn bare(id: String) -> Self {
        Self {
            id,
            adapter: None,
            context_window: None,
            max_output_tokens: None,
            reasoning_levels: None,
            reasoning_map: None,
            thinking_format: None,
            requires_reasoning_content: None,
            default_reasoning: None,
        }
    }

    pub fn normalize(&mut self) {
        if self.reasoning_levels.is_none() {
            if let Some(map) = &self.reasoning_map {
                self.reasoning_levels = Some(map.keys().cloned().collect());
            }
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty()
            || self.adapter.as_deref() == Some("")
            || self.context_window == Some(0)
            || self.max_output_tokens == Some(0)
        {
            return Err(format!("invalid model metadata for {:?}", self.id));
        }
        if let Some(map) = &self.reasoning_map {
            if map
                .iter()
                .any(|(key, value)| key.is_empty() || value.is_empty())
            {
                return Err(format!("invalid reasoning map for {}", self.id));
            }
        }
        if let Some(levels) = &self.reasoning_levels {
            if levels.iter().any(String::is_empty)
                || levels.len() != levels.iter().collect::<HashSet<_>>().len()
            {
                return Err(format!("invalid reasoning levels for {}", self.id));
            }
            if let Some(map) = &self.reasoning_map {
                if levels.iter().any(|level| !map.contains_key(level)) {
                    return Err(format!("reasoning levels missing mappings for {}", self.id));
                }
            }
        }
        if let Some(level) = &self.default_reasoning {
            if self
                .reasoning_levels
                .as_ref()
                .is_none_or(|levels| !levels.contains(level))
            {
                return Err(format!(
                    "default reasoning level {level} is not supported by {}",
                    self.id
                ));
            }
        }
        Ok(())
    }
}
