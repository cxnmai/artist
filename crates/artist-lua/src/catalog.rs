//! Optional OpenAI-style model discovery, with Lua parsing for richer catalogs.

use crate::model::provider_headers;
use crate::model_info::ModelInfo;
use crate::registry::{ModelKind, RegisteredProvider};
use artist_model::http::HttpClient;
use mlua::{Lua, LuaSerdeExt};
use serde_json::Value;
use std::collections::BTreeMap;

pub async fn list(lua: &Lua, provider: &RegisteredProvider) -> Result<Vec<ModelInfo>, String> {
    let mut models = BTreeMap::new();
    if let Some(discover) = &provider.discover {
        if !matches!(provider.kind, ModelKind::Http { .. }) {
            return Err("model discovery currently requires chat_completions".into());
        }
        let response = HttpClient::new()
            .get(&discover.endpoint, provider_headers(&provider.kind)?)
            .await
            .map_err(|e| format!("model discovery: {e}"))?;
        if !(200..300).contains(&response.status) {
            return Err(format!(
                "model discovery HTTP {}: {}",
                response.status,
                String::from_utf8_lossy(&response.body)
            ));
        }
        let body: Value = serde_json::from_slice(&response.body)
            .map_err(|e| format!("model discovery JSON: {e}"))?;
        let discovered: Vec<ModelInfo> = if let Some(parse) = &discover.parse {
            let value = lua.to_value(&body).map_err(|e| e.to_string())?;
            let entries = parse
                .call(value)
                .map_err(|e| format!("model discovery parser: {e}"))?;
            lua.from_value(entries)
                .map_err(|e| format!("model discovery entries: {e}"))?
        } else {
            body.get("data")
                .and_then(Value::as_array)
                .ok_or("model discovery expected a data array")?
                .iter()
                .map(|item| {
                    item.get("id")
                        .and_then(Value::as_str)
                        .map(|id| ModelInfo::bare(id.into()))
                        .ok_or_else(|| "model discovery entry missing id".to_owned())
                })
                .collect::<Result<_, _>>()?
        };
        for mut info in discovered {
            info.normalize();
            info.validate()?;
            if let Some(filter) = &discover.filter {
                if !filter
                    .call::<bool>(info.id.as_str())
                    .map_err(|e| e.to_string())?
                {
                    continue;
                }
            }
            if models.insert(info.id.clone(), info).is_some() {
                return Err("duplicate model ID in discovery response".into());
            }
        }
    }
    // Explicit metadata wins over a discovered entry with the same ID.
    for id in &provider.models {
        let mut info = models
            .remove(id)
            .unwrap_or_else(|| ModelInfo::bare(id.clone()));
        if let Some(override_info) = provider.model_info.get(id) {
            if let Some(adapter) = &override_info.adapter {
                info.adapter = Some(adapter.clone());
            }
            if let Some(window) = override_info.context_window {
                info.context_window = Some(window);
            }
            if let Some(max) = override_info.max_output_tokens {
                info.max_output_tokens = Some(max);
            }
            if let Some(levels) = &override_info.reasoning_levels {
                info.reasoning_levels = Some(levels.clone());
            }
            if let Some(map) = &override_info.reasoning_map {
                info.reasoning_map = Some(map.clone());
                if override_info.reasoning_levels.is_none() {
                    info.reasoning_levels = None;
                }
            }
            if let Some(format) = &override_info.thinking_format {
                info.thinking_format = Some(format.clone());
            }
            if let Some(required) = override_info.requires_reasoning_content {
                info.requires_reasoning_content = Some(required);
            }
            if let Some(level) = &override_info.default_reasoning {
                info.default_reasoning = Some(level.clone());
            }
        }
        info.normalize();
        info.validate()?;
        models.insert(id.clone(), info);
    }
    Ok(models.into_values().collect())
}
