//! Optional OpenAI-style model catalog discovery; never fetched during a normal prompt.

use crate::model::provider_headers;
use crate::registry::{ModelKind, RegisteredProvider};
use artist_model::http::HttpClient;
use serde_json::Value;

pub async fn list(provider: &RegisteredProvider) -> Result<Vec<String>, String> {
    let mut models = provider.models.clone();
    if let Some(discover) = &provider.discover {
        if !matches!(provider.kind, ModelKind::ChatCompletions { .. }) {
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
        let data = body
            .get("data")
            .and_then(Value::as_array)
            .ok_or("model discovery expected a data array")?;
        for item in data {
            let id = item
                .get("id")
                .and_then(Value::as_str)
                .ok_or("model discovery entry missing id")?;
            if let Some(filter) = &discover.filter {
                if !filter.call::<bool>(id).map_err(|e| e.to_string())? {
                    continue;
                }
            }
            models.push(id.into());
        }
    }
    models.sort();
    models.dedup();
    Ok(models)
}
