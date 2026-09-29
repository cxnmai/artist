//! Buffered Anthropic Messages protocol with signed thinking-block replay.

use super::sse::Sse;
use crate::ModelInput;
use crate::adapter::Adapter;
use crate::http::{HeaderMap, HeaderName, HeaderValue, Method, Request, Response};
use artist_core::context::{AssistantBlock, ContextEntry};
use artist_core::event::ModelEvent;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct MessageStream {
    message: Option<Value>,
    blocks: BTreeMap<u64, Value>,
    arguments: BTreeMap<u64, String>,
    text: String,
    thinking: String,
    done: bool,
}

#[derive(Default)]
pub struct AnthropicMessages {
    pub name: String,
    pub endpoint: String,
    pub headers: HeaderMap,
    pub options: serde_json::Map<String, Value>,
    pub reasoning_level: Option<String>,
    pub stream: bool,
    pub(crate) sse: Sse,
    pub(crate) stream_state: MessageStream,
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
        body.insert("stream".into(), json!(self.stream));
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

    fn stream_start(&mut self, status: u16, _: &HeaderMap) -> Result<Vec<ModelEvent>, String> {
        if !(200..300).contains(&status) {
            return Err(format!("Messages HTTP {status}"));
        }
        Ok(Vec::new())
    }

    fn stream_chunk(&mut self, chunk: &[u8]) -> Result<Vec<ModelEvent>, String> {
        let mut events = Vec::new();
        for (event_name, frame) in self.sse.push(chunk)? {
            let kind = frame
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or(&event_name);
            match kind {
                "message_start" => {
                    self.stream_state.message = Some(
                        frame
                            .get("message")
                            .ok_or("message_start missing message")?
                            .clone(),
                    );
                }
                "content_block_start" => {
                    let index = frame
                        .get("index")
                        .and_then(Value::as_u64)
                        .ok_or("content_block_start missing index")?;
                    let block = frame
                        .get("content_block")
                        .ok_or("content_block_start missing content_block")?
                        .clone();
                    if self.stream_state.blocks.insert(index, block).is_some() {
                        return Err("duplicate content block".into());
                    }
                }
                "content_block_delta" => {
                    let index = frame
                        .get("index")
                        .and_then(Value::as_u64)
                        .ok_or("content_block_delta missing index")?;
                    let block = self
                        .stream_state
                        .blocks
                        .get_mut(&index)
                        .ok_or("delta for unknown content block")?;
                    let delta = frame
                        .get("delta")
                        .ok_or("content_block_delta missing delta")?;
                    match delta.get("type").and_then(Value::as_str) {
                        Some("text_delta") => {
                            let text = delta
                                .get("text")
                                .and_then(Value::as_str)
                                .ok_or("text_delta missing text")?;
                            let current = block.get("text").and_then(Value::as_str).unwrap_or("");
                            block["text"] = json!(format!("{current}{text}"));
                            self.stream_state.text.push_str(text);
                            events.push(ModelEvent::TextDelta { text: text.into() });
                        }
                        Some("thinking_delta") => {
                            let text = delta
                                .get("thinking")
                                .and_then(Value::as_str)
                                .ok_or("thinking_delta missing thinking")?;
                            let current =
                                block.get("thinking").and_then(Value::as_str).unwrap_or("");
                            block["thinking"] = json!(format!("{current}{text}"));
                            self.stream_state.thinking.push_str(text);
                            events.push(ModelEvent::ThinkingDelta { text: text.into() });
                        }
                        Some("signature_delta") => {
                            block["signature"] = delta
                                .get("signature")
                                .cloned()
                                .ok_or("signature_delta missing signature")?;
                        }
                        Some("input_json_delta") => {
                            let part = delta
                                .get("partial_json")
                                .and_then(Value::as_str)
                                .ok_or("input_json_delta missing partial_json")?;
                            self.stream_state
                                .arguments
                                .entry(index)
                                .or_default()
                                .push_str(part);
                        }
                        _ => {}
                    }
                }
                "content_block_stop" => {
                    let index = frame
                        .get("index")
                        .and_then(Value::as_u64)
                        .ok_or("content_block_stop missing index")?;
                    if let Some(args) = self.stream_state.arguments.remove(&index) {
                        let block = self
                            .stream_state
                            .blocks
                            .get_mut(&index)
                            .ok_or("stop for unknown content block")?;
                        block["input"] = serde_json::from_str(&args)
                            .map_err(|e| format!("stream tool input: {e}"))?;
                    }
                }
                "message_delta" => {
                    let message = self
                        .stream_state
                        .message
                        .as_mut()
                        .ok_or("message_delta before message_start")?;
                    if let Some(reason) = frame.pointer("/delta/stop_reason") {
                        message["stop_reason"] = reason.clone();
                    }
                    if let Some(usage) = frame.get("usage") {
                        if message.get("usage").is_none() {
                            message["usage"] = json!({});
                        }
                        if let (Some(current), Some(fields)) =
                            (message["usage"].as_object_mut(), usage.as_object())
                        {
                            current.extend(fields.clone());
                        }
                    }
                }
                "message_stop" => {
                    if self.stream_state.done {
                        return Err("duplicate message_stop".into());
                    }
                    self.stream_state.done = true;
                    let mut message = self
                        .stream_state
                        .message
                        .take()
                        .ok_or("message_stop before message_start")?;
                    message["content"] = json!(
                        std::mem::take(&mut self.stream_state.blocks)
                            .into_values()
                            .collect::<Vec<_>>()
                    );
                    let body = serde_json::to_vec(&message).map_err(|e| e.to_string())?;
                    let final_events = self.response(Response {
                        status: 200,
                        headers: HeaderMap::new(),
                        body,
                    })?;
                    let final_text = final_events
                        .iter()
                        .filter_map(|event| match event {
                            ModelEvent::TextDelta { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<String>();
                    if !self.stream_state.text.is_empty() && final_text != self.stream_state.text {
                        return Err("Messages streamed text differs from final blocks".into());
                    }
                    for event in final_events {
                        match &event {
                            ModelEvent::TextDelta { .. } if !self.stream_state.text.is_empty() => {}
                            ModelEvent::ThinkingDelta { .. }
                                if !self.stream_state.thinking.is_empty() => {}
                            _ => events.push(event),
                        }
                    }
                }
                "error" => return Err(format!("Messages stream error: {frame}")),
                _ => {}
            }
        }
        Ok(events)
    }

    fn stream_end(&mut self) -> Result<Vec<ModelEvent>, String> {
        self.sse.finish()?;
        if !self.stream_state.done {
            return Err("Messages stream ended without message_stop".into());
        }
        Ok(Vec::new())
    }
}
