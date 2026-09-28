use crate::registry::ModelCallbacks;
use artist_core::context::SelectedContext;
use artist_core::engine::Model;
use artist_core::event::ModelEvent;
use artist_core::tools::ToolDefinition;
use artist_model::ModelInput;
use artist_model::adapter::Adapter;
use artist_model::http::{HeaderMap, HeaderName, HeaderValue, Method, Request, Response};
use mlua::{Lua, LuaSerdeExt, Value as LuaValue};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

#[derive(Deserialize)]
struct RequestSpec {
    url: String,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    body: Value,
    timeout: Option<f64>,
}

struct LuaAdapter<'a> {
    lua: &'a Lua,
    callbacks: &'a ModelCallbacks,
}

impl Adapter for LuaAdapter<'_> {
    type Error = String;

    fn request(&self, input: &ModelInput<'_>) -> Result<Request, String> {
        let context = self
            .lua
            .to_value(&serde_json::json!({
                "system_prompt": input.system_prompt,
                "entries": input.entries,
            }))
            .map_err(|e| e.to_string())?;
        let tools = self.lua.to_value(input.tools).map_err(|e| e.to_string())?;
        let value: LuaValue = self
            .callbacks
            .request
            .call((context, tools))
            .map_err(|e| e.to_string())?;
        let spec: RequestSpec = self
            .lua
            .from_value(value)
            .map_err(|e| format!("request: {e}"))?;
        let method = spec
            .method
            .as_deref()
            .unwrap_or("POST")
            .parse::<Method>()
            .map_err(|e| e.to_string())?;
        let mut headers = HeaderMap::new();
        for (name, value) in spec.headers {
            headers.insert(
                HeaderName::from_bytes(name.as_bytes()).map_err(|e| e.to_string())?,
                HeaderValue::from_str(&value).map_err(|e| e.to_string())?,
            );
        }
        let timeout = spec
            .timeout
            .map(|seconds| {
                if !seconds.is_finite() || seconds <= 0.0 || seconds >= u64::MAX as f64 {
                    Err("timeout must be a positive finite number of seconds".to_owned())
                } else {
                    Ok(Duration::from_secs_f64(seconds))
                }
            })
            .transpose()?;
        Ok(Request {
            method,
            url: spec.url,
            headers,
            body: spec.body,
            timeout,
        })
    }

    fn response(&mut self, response: Response) -> Result<Vec<ModelEvent>, String> {
        let body: Value = serde_json::from_slice(&response.body).map_err(|e| e.to_string())?;
        let headers: BTreeMap<_, _> = response
            .headers
            .iter()
            .filter_map(|(name, value)| value.to_str().ok().map(|value| (name.as_str(), value)))
            .collect();
        let headers = self.lua.to_value(&headers).map_err(|e| e.to_string())?;
        let body = self.lua.to_value(&body).map_err(|e| e.to_string())?;
        let events: LuaValue = self
            .callbacks
            .response
            .call((response.status, headers, body))
            .map_err(|e| e.to_string())?;
        let events: Value = self
            .lua
            .from_value(events)
            .map_err(|e| format!("response events: {e}"))?;
        serde_json::from_value(events).map_err(|e| format!("response events: {e}"))
    }

    fn stream_chunk(&mut self, _: &[u8]) -> Result<Vec<ModelEvent>, String> {
        Err("streaming model responses are not configured".into())
    }
}

pub struct LuaModel<'a> {
    pub lua: &'a Lua,
    pub callbacks: ModelCallbacks,
    pub client: artist_model::ModelClient,
}

impl Model for LuaModel<'_> {
    fn generate<'a>(
        &'a mut self,
        context: &'a SelectedContext,
        tools: &'a [ToolDefinition],
        emit: &'a mut dyn FnMut(ModelEvent),
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + 'a>> {
        Box::pin(async move {
            let mut adapter = LuaAdapter {
                lua: self.lua,
                callbacks: &self.callbacks,
            };
            let input = ModelInput {
                system_prompt: &context.system_prompt,
                entries: &context.entries,
                tools,
            };
            let events = self
                .client
                .complete(&mut adapter, &input)
                .await
                .map_err(|error| match error {
                    artist_model::ModelError::Transport(error) => error.to_string(),
                    artist_model::ModelError::Adapter(error) => error,
                })?;
            for event in events {
                emit(event);
            }
            Ok(())
        })
    }
}
