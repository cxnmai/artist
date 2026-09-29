use crate::registry::{ModelKind, Protocol, RegisteredProvider};
use artist_core::context::SelectedContext;
use artist_core::engine::Model;
use artist_core::event::ModelEvent;
use artist_core::tools::ToolDefinition;
use artist_model::ModelInput;
use artist_model::adapter::Adapter;
use artist_model::adapters::anthropic_messages::AnthropicMessages;
use artist_model::adapters::chat_completions::{ChatCompletions, bearer_header};
use artist_model::adapters::openai_responses::OpenAIResponses;
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
    request: &'a mlua::Function,
    response: &'a mlua::Function,
    model_name: &'a str,
    reasoning_level: Option<&'a str>,
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
            .request
            .call((context, tools, self.model_name, self.reasoning_level))
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

async fn generate<A: Adapter<Error = String>>(
    client: &artist_model::ModelClient,
    adapter: &mut A,
    input: &ModelInput<'_>,
    stream: bool,
    emit: &mut dyn FnMut(Vec<ModelEvent>),
) -> Result<Vec<ModelEvent>, String> {
    let convert = |error| match error {
        artist_model::ModelError::Transport(error) => error.to_string(),
        artist_model::ModelError::Adapter(error) => error,
    };
    if !stream {
        return client.complete(adapter, input).await.map_err(convert);
    }
    let mut pending = Vec::new();
    client
        .stream(adapter, input, |events| {
            let mut live = Vec::new();
            for event in events {
                match event {
                    ModelEvent::TextDelta { .. } | ModelEvent::ThinkingDelta { .. } => {
                        live.push(event)
                    }
                    _ => pending.push(event),
                }
            }
            if !live.is_empty() {
                emit(live);
            }
        })
        .await
        .map_err(convert)?;
    Ok(pending)
}

pub fn provider_headers(kind: &ModelKind) -> Result<HeaderMap, String> {
    let ModelKind::Http {
        bearer_env,
        api_key_env,
        headers,
        ..
    } = kind
    else {
        return Ok(HeaderMap::new());
    };
    let mut header_map = HeaderMap::new();
    if let Some(env_name) = bearer_env {
        let token = std::env::var(env_name)
            .map_err(|_| format!("environment variable {env_name} is not set"))?;
        header_map.insert(
            HeaderName::from_static("authorization"),
            bearer_header(&token)?,
        );
    }
    if let Some(env_name) = api_key_env {
        let token = std::env::var(env_name)
            .map_err(|_| format!("environment variable {env_name} is not set"))?;
        header_map.insert(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_str(&token).map_err(|e| e.to_string())?,
        );
    }
    if let Some(headers) = headers {
        for pair in headers.clone().pairs::<String, LuaValue>() {
            let (key, value) = pair.map_err(|e| e.to_string())?;
            let value: String = match value {
                LuaValue::String(text) => text.to_str().map_err(|e| e.to_string())?.to_owned(),
                LuaValue::Function(callback) => callback.call(()).map_err(|e| e.to_string())?,
                _ => return Err(format!("header {key} must be a string or function")),
            };
            header_map.insert(
                HeaderName::from_bytes(key.as_bytes()).map_err(|e| e.to_string())?,
                HeaderValue::from_str(&value).map_err(|e| e.to_string())?,
            );
        }
    }
    Ok(header_map)
}

pub struct LuaModel<'a> {
    pub lua: &'a Lua,
    pub provider: RegisteredProvider,
    pub model_name: String,
    pub adapter_name: Option<String>,
    pub reasoning_level: Option<String>,
    pub thinking_format: Option<String>,
    pub requires_reasoning_content: bool,
    pub client: artist_model::ModelClient,
}

impl Model for LuaModel<'_> {
    fn generate<'a>(
        &'a mut self,
        context: &'a SelectedContext,
        tools: &'a [ToolDefinition],
        emit: &'a mut dyn FnMut(Vec<ModelEvent>),
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + 'a>> {
        Box::pin(async move {
            let input = ModelInput {
                system_prompt: &context.system_prompt,
                entries: &context.entries,
                tools,
            };
            let kind = self
                .adapter_name
                .as_ref()
                .map(|name| {
                    self.provider
                        .adapters
                        .get(name)
                        .ok_or_else(|| format!("unknown adapter {name}"))
                })
                .transpose()?
                .unwrap_or(&self.provider.kind);
            let mut events = match kind {
                ModelKind::Lua { request, response } => {
                    let mut adapter = LuaAdapter {
                        lua: self.lua,
                        request,
                        response,
                        model_name: &self.model_name,
                        reasoning_level: self.reasoning_level.as_deref(),
                    };
                    generate(&self.client, &mut adapter, &input, false, emit).await?
                }
                ModelKind::Http {
                    protocol,
                    endpoint,
                    options,
                    stream,
                    ..
                } => {
                    let headers = provider_headers(kind)?;
                    match protocol {
                        Protocol::ChatCompletions => {
                            let mut adapter = ChatCompletions::default();
                            adapter.name = self.model_name.clone();
                            adapter.endpoint = endpoint.clone();
                            adapter.headers = headers;
                            adapter.options = options.clone();
                            adapter.reasoning_level = self.reasoning_level.clone();
                            adapter.thinking_format = self.thinking_format.clone();
                            adapter.requires_reasoning_content = self.requires_reasoning_content;
                            adapter.stream = *stream;
                            generate(&self.client, &mut adapter, &input, *stream, emit).await?
                        }
                        Protocol::OpenAIResponses => {
                            let mut adapter = OpenAIResponses::default();
                            adapter.name = self.model_name.clone();
                            adapter.endpoint = endpoint.clone();
                            adapter.headers = headers;
                            adapter.options = options.clone();
                            adapter.reasoning_level = self.reasoning_level.clone();
                            adapter.stream = *stream;
                            generate(&self.client, &mut adapter, &input, *stream, emit).await?
                        }
                        Protocol::AnthropicMessages => {
                            let mut adapter = AnthropicMessages::default();
                            adapter.name = self.model_name.clone();
                            adapter.endpoint = endpoint.clone();
                            adapter.headers = headers;
                            adapter.options = options.clone();
                            adapter.reasoning_level = self.reasoning_level.clone();
                            adapter.stream = *stream;
                            generate(&self.client, &mut adapter, &input, *stream, emit).await?
                        }
                    }
                }
            };
            if let Some(cost) = &self.provider.cost {
                for event in &mut events {
                    if let ModelEvent::Usage {
                        input_tokens,
                        output_tokens,
                        total_tokens,
                        cached_input_tokens,
                        cache_write_input_tokens,
                        reasoning_output_tokens,
                        cost_usd,
                    } = event
                    {
                        if cost_usd.is_some() {
                            continue;
                        }
                        let usage = self.lua.create_table().map_err(|e| e.to_string())?;
                        usage
                            .set("input_tokens", *input_tokens)
                            .map_err(|e| e.to_string())?;
                        usage
                            .set("output_tokens", *output_tokens)
                            .map_err(|e| e.to_string())?;
                        usage
                            .set("total_tokens", *total_tokens)
                            .map_err(|e| e.to_string())?;
                        usage
                            .set("cached_input_tokens", *cached_input_tokens)
                            .map_err(|e| e.to_string())?;
                        usage
                            .set("cache_write_input_tokens", *cache_write_input_tokens)
                            .map_err(|e| e.to_string())?;
                        usage
                            .set("reasoning_output_tokens", *reasoning_output_tokens)
                            .map_err(|e| e.to_string())?;
                        let value: Option<f64> = cost
                            .call((self.model_name.as_str(), usage))
                            .map_err(|e| format!("cost calculation: {e}"))?;
                        if let Some(value) = value {
                            if !value.is_finite() || value < 0.0 {
                                return Err("cost calculation must return a nonnegative finite USD amount or nil".into());
                            }
                            *cost_usd = Some(value);
                        }
                    }
                }
            }
            if !events.is_empty() {
                emit(events);
            }
            Ok(())
        })
    }
}
