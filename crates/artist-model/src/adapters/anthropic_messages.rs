//! Buffered Anthropic Messages protocol with signed thinking-block replay.

use crate::ModelInput;
use crate::adapter::Adapter;
use crate::http::{HeaderMap, HeaderName, HeaderValue, Method, Request, Response};
use artist_core::context::{AssistantBlock, ContextEntry};
use artist_core::event::ModelEvent;
use serde_json::{Value, json};

pub struct AnthropicMessages {
    pub name: String,
    pub endpoint: String,
    pub headers: HeaderMap,
    pub options: serde_json::Map<String, Value>,
    pub reasoning_level: Option<String>,
}

impl Adapter for AnthropicMessages {
    type Error = String;

    fn request(&self, context: &ModelInput<'_>) -> Result<Request, String> {
        let mut messages: Vec<Value> = Vec::new();
        for entry in context.entries {
            match entry {
                ContextEntry::User { text } => {
                    messages.push(json!({"role":"user","content":[{"type":"text","text":text}]}))
                }
                ContextEntry::ToolResult {
                    tool_call_id,
                    text,
                    is_error,
                } => {
                    let block = json!({"type":"tool_result","tool_use_id":tool_call_id,"content":text,"is_error":is_error});
                    if let Some(last) = messages.last_mut().filter(|m| {
                        m["role"] == "user"
                            && m["content"].as_array().is_some_and(|parts| {
                                parts.iter().all(|p| p["type"] == "tool_result")
                            })
                    }) {
                        last["content"].as_array_mut().unwrap().push(block);
                    } else {
                        messages.push(json!({"role":"user","content":[block]}));
                    }
                }
                ContextEntry::Assistant { blocks } => {
                    let native: Vec<_> = blocks
                        .iter()
                        .filter_map(|b| match b {
                            AssistantBlock::ProviderData { provider, data }
                                if provider == "anthropic_messages" =>
                            {
                                Some(data.clone())
                            }
                            _ => None,
                        })
                        .collect();
                    let parts = if !native.is_empty() {
                        native
                    } else {
                        blocks.iter().filter_map(|block| match block {
                            AssistantBlock::Text { text } => Some(json!({"type":"text","text":text})),
                            AssistantBlock::ToolCall { id, name, arguments } => Some(json!({"type":"tool_use","id":id,"name":name,"input":arguments})),
                            // Unsigned thinking cannot be replayed to Anthropic.
                            _ => None,
                        }).collect()
                    };
                    if !parts.is_empty() {
                        messages.push(json!({"role":"assistant","content":parts}));
                    }
                }
            }
        }
        let mut body = self.options.clone();
        body.insert("model".into(), json!(self.name));
        body.insert("messages".into(), json!(messages));
        body.entry("max_tokens").or_insert_with(|| json!(8192));
        body.insert("stream".into(), json!(false));
        if !context.system_prompt.is_empty() {
            body.insert("system".into(), json!(context.system_prompt));
        } else {
            body.remove("system");
        }
        if let Some(level) = &self.reasoning_level {
            // Anthropic effort support is model-specific; the registry declares supported values.
            let output = body.entry("output_config").or_insert_with(|| json!({}));
            output
                .as_object_mut()
                .ok_or("Messages output_config must be an object")?
                .insert("effort".into(), json!(level));
        }
        body.remove("tools");
        if !context.tools.is_empty() {
            body.insert("tools".into(), Value::Array(context.tools.iter().map(|tool| json!({
                "name":tool.name,"description":tool.description,"input_schema":tool.parameters
            })).collect()));
        }
        let mut headers = self.headers.clone();
        if !headers.contains_key("anthropic-version") {
            headers.insert(
                HeaderName::from_static("anthropic-version"),
                HeaderValue::from_static("2023-06-01"),
            );
        }
        Ok(Request {
            method: Method::POST,
            url: self.endpoint.clone(),
            headers,
            body: Value::Object(body),
            timeout: None,
        })
    }

    fn response(&mut self, response: Response) -> Result<Vec<ModelEvent>, String> {
        if !(200..300).contains(&response.status) {
            return Err(format!(
                "Messages HTTP {}: {}",
                response.status,
                String::from_utf8_lossy(&response.body)
            ));
        }
        let body: Value =
            serde_json::from_slice(&response.body).map_err(|e| format!("Messages JSON: {e}"))?;
        let blocks = body
            .get("content")
            .and_then(Value::as_array)
            .ok_or("Messages response missing content")?;
        let mut events = Vec::new();
        for block in blocks {
            let kind = block
                .get("type")
                .and_then(Value::as_str)
                .ok_or("Messages block missing type")?;
            events.push(ModelEvent::ProviderData {
                provider: "anthropic_messages".into(),
                data: block.clone(),
            });
            match kind {
                "text" => {
                    let text = block
                        .get("text")
                        .and_then(Value::as_str)
                        .ok_or("Messages text block missing text")?;
                    events.push(ModelEvent::TextDelta { text: text.into() });
                }
                "thinking" => {
                    let text = block
                        .get("thinking")
                        .and_then(Value::as_str)
                        .ok_or("Messages thinking block missing thinking")?;
                    events.push(ModelEvent::ThinkingDelta { text: text.into() });
                }
                "tool_use" => {
                    let id = block
                        .get("id")
                        .and_then(Value::as_str)
                        .ok_or("Messages tool_use missing id")?
                        .to_owned();
                    let name = block
                        .get("name")
                        .and_then(Value::as_str)
                        .ok_or("Messages tool_use missing name")?
                        .to_owned();
                    let args = block
                        .get("input")
                        .ok_or("Messages tool_use missing input")?
                        .to_string();
                    events.push(ModelEvent::ToolCallStart {
                        id: id.clone(),
                        name,
                    });
                    events.push(ModelEvent::ToolCallArgumentsDelta {
                        id: id.clone(),
                        text: args,
                    });
                    events.push(ModelEvent::ToolCallEnd { id });
                }
                _ => {} // Redacted thinking and provider blocks are retained verbatim.
            }
        }
        if let Some(usage) = body.get("usage") {
            if let (Some(input), Some(output_tokens)) = (
                usage.get("input_tokens").and_then(Value::as_u64),
                usage.get("output_tokens").and_then(Value::as_u64),
            ) {
                let cached = usage.get("cache_read_input_tokens").and_then(Value::as_u64);
                let created = usage
                    .get("cache_creation_input_tokens")
                    .and_then(Value::as_u64);
                let input_tokens = input
                    .saturating_add(cached.unwrap_or(0))
                    .saturating_add(created.unwrap_or(0));
                events.push(ModelEvent::Usage {
                    input_tokens,
                    output_tokens,
                    total_tokens: None,
                    cached_input_tokens: cached,
                    cache_write_input_tokens: created,
                    reasoning_output_tokens: None,
                    cost_usd: None,
                });
            }
        }
        events.push(ModelEvent::Finished {
            reason: body
                .get("stop_reason")
                .and_then(Value::as_str)
                .map(str::to_owned),
        });
        Ok(events)
    }

    fn stream_chunk(&mut self, _: &[u8]) -> Result<Vec<ModelEvent>, String> {
        Err("Messages streaming is not configured".into())
    }
}
