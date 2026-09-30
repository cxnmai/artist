//! Buffered OpenAI-style Chat Completions protocol. Endpoint/auth are supplied by the caller.

use super::sse::Sse;
use crate::ModelInput;
use crate::adapter::Adapter;
use crate::http::{HeaderMap, HeaderValue, Method, Request, Response};
use artist_core::context::{AssistantBlock, ContextEntry};
use artist_core::event::ModelEvent;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct ChatStream {
    done: bool,
    calls: BTreeMap<u64, StreamCall>,
    reason: Option<String>,
    usage: Option<ModelEvent>,
}

#[derive(Default)]
struct StreamCall {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Default)]
pub struct ChatCompletions {
    pub name: String,
    pub endpoint: String,
    pub headers: HeaderMap,
    pub options: serde_json::Map<String, Value>,
    pub reasoning_level: Option<String>,
    pub thinking_format: Option<String>,
    pub requires_reasoning_content: bool,
    pub stream: bool,
    pub(crate) sse: Sse,
    pub(crate) stream_state: ChatStream,
}

impl Adapter for ChatCompletions {
    type Error = String;

    fn request(&self, input: &ModelInput<'_>) -> Result<Request, String> {
        let mut messages = Vec::new();
        if !input.system_prompt.is_empty() {
            messages.push(json!({"role": "system", "content": input.system_prompt}));
        }
        for entry in input.entries {
            match entry {
                ContextEntry::User { text } | ContextEntry::Summary { text } => {
                    messages.push(json!({"role": "user", "content": text}))
                }
                ContextEntry::ToolResult {
                    tool_call_id, text, ..
                } => messages
                    .push(json!({"role": "tool", "tool_call_id": tool_call_id, "content": text})),
                ContextEntry::Assistant { blocks } => {
                    let mut text = String::new();
                    let mut thinking = String::new();
                    let mut calls = Vec::new();
                    for block in blocks {
                        match block {
                            AssistantBlock::Text { text: part } => text.push_str(part),
                            AssistantBlock::Thinking { text: part } => thinking.push_str(part),
                            AssistantBlock::ProviderData { .. } => {}
                            AssistantBlock::ToolCall {
                                id,
                                name,
                                arguments,
                            } => calls.push(json!({
                                "id": id, "type": "function",
                                "function": {"name": name, "arguments": arguments.to_string()}
                            })),
                        }
                    }
                    let mut message = json!({"role": "assistant", "content": text});
                    if !thinking.is_empty() || self.requires_reasoning_content {
                        message["reasoning_content"] = json!(thinking);
                    }
                    if !calls.is_empty() {
                        message["tool_calls"] = json!(calls);
                    }
                    messages.push(message);
                }
            }
        }
        let tools: Vec<Value> = input.tools.iter().map(|tool| json!({
            "type": "function", "function": {
                "name": tool.name, "description": tool.description, "parameters": tool.parameters
            }
        })).collect();
        let mut body = self.options.clone();
        body.insert("model".into(), json!(self.name));
        body.insert("messages".into(), json!(messages));
        body.insert("stream".into(), json!(self.stream));
        if self.stream {
            body.insert("stream_options".into(), json!({"include_usage": true}));
        }
        if let Some(level) = &self.reasoning_level {
            if self.thinking_format.as_deref() == Some("deepseek") {
                body.insert(
                    "thinking".into(),
                    json!({"type": if level == "none" { "disabled" } else { "enabled" }}),
                );
                if level != "none" {
                    body.insert("reasoning_effort".into(), json!(level));
                } else {
                    body.remove("reasoning_effort");
                }
            } else {
                body.insert("reasoning_effort".into(), json!(level));
            }
        }
        body.remove("tools");
        if !tools.is_empty() {
            body.insert("tools".into(), json!(tools));
        }
        Ok(Request {
            method: Method::POST,
            url: self.endpoint.clone(),
            headers: self.headers.clone(),
            body: Value::Object(body),
            timeout: None,
        })
    }

    fn response(&mut self, response: Response) -> Result<Vec<ModelEvent>, String> {
        if !(200..300).contains(&response.status) {
            return Err(format!(
                "chat completions HTTP {}: {}",
                response.status,
                String::from_utf8_lossy(&response.body)
            ));
        }
        let body: Value = serde_json::from_slice(&response.body)
            .map_err(|e| format!("chat completions response JSON: {e}"))?;
        let choice = body
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .ok_or("chat completions response has no choice")?;
        let message = choice
            .get("message")
            .ok_or("chat completions response has no message")?;
        let mut events = Vec::new();
        if let Some(text) = message
            .get("reasoning_content")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            events.push(ModelEvent::ThinkingDelta { text: text.into() });
        }
        if let Some(text) = message
            .get("content")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            events.push(ModelEvent::TextDelta { text: text.into() });
        }
        if let Some(calls) = message.get("tool_calls") {
            for call in calls.as_array().ok_or("tool_calls is not an array")? {
                let id = call
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or("tool call missing id")?
                    .to_owned();
                let function = call.get("function").ok_or("tool call missing function")?;
                let name = function
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or("tool call missing name")?
                    .to_owned();
                let arguments = function
                    .get("arguments")
                    .and_then(Value::as_str)
                    .ok_or("tool call missing arguments")?
                    .to_owned();
                events.push(ModelEvent::ToolCallStart {
                    id: id.clone(),
                    name,
                });
                events.push(ModelEvent::ToolCallArgumentsDelta {
                    id: id.clone(),
                    text: arguments,
                });
                events.push(ModelEvent::ToolCallEnd { id });
            }
        }
        if let Some(usage) = body.get("usage") {
            if let (Some(input_tokens), Some(output_tokens)) = (
                usage.get("prompt_tokens").and_then(Value::as_u64),
                usage.get("completion_tokens").and_then(Value::as_u64),
            ) {
                events.push(ModelEvent::Usage {
                    input_tokens,
                    output_tokens,
                    total_tokens: usage.get("total_tokens").and_then(Value::as_u64),
                    cached_input_tokens: usage
                        .pointer("/prompt_tokens_details/cached_tokens")
                        .and_then(Value::as_u64),
                    cache_write_input_tokens: None,
                    reasoning_output_tokens: usage
                        .pointer("/completion_tokens_details/reasoning_tokens")
                        .and_then(Value::as_u64),
                    cost_usd: None,
                });
            }
        }
        events.push(ModelEvent::Finished {
            reason: choice
                .get("finish_reason")
                .and_then(Value::as_str)
                .map(str::to_owned),
        });
        Ok(events)
    }

    fn stream_start(&mut self, status: u16, _: &HeaderMap) -> Result<Vec<ModelEvent>, String> {
        if !(200..300).contains(&status) {
            return Err(format!("Chat Completions HTTP {status}"));
        }
        Ok(Vec::new())
    }

    fn stream_chunk(&mut self, chunk: &[u8]) -> Result<Vec<ModelEvent>, String> {
        let mut events = Vec::new();
        for (_, frame) in self.sse.push(chunk)? {
            if frame.is_null() {
                self.stream_state.done = true;
                continue;
            }
            if let Some(error) = frame.get("error") {
                return Err(format!("Chat Completions stream error: {error}"));
            }
            if let Some(usage) = frame.get("usage").filter(|v| !v.is_null()) {
                if let (Some(input_tokens), Some(output_tokens)) = (
                    usage.get("prompt_tokens").and_then(Value::as_u64),
                    usage.get("completion_tokens").and_then(Value::as_u64),
                ) {
                    self.stream_state.usage = Some(ModelEvent::Usage {
                        input_tokens,
                        output_tokens,
                        total_tokens: usage.get("total_tokens").and_then(Value::as_u64),
                        cached_input_tokens: usage
                            .pointer("/prompt_tokens_details/cached_tokens")
                            .and_then(Value::as_u64),
                        cache_write_input_tokens: None,
                        reasoning_output_tokens: usage
                            .pointer("/completion_tokens_details/reasoning_tokens")
                            .and_then(Value::as_u64),
                        cost_usd: None,
                    });
                }
            }
            if let Some(choice) = frame
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
            {
                if let Some(delta) = choice.get("delta") {
                    if let Some(text) = delta
                        .get("content")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                    {
                        events.push(ModelEvent::TextDelta { text: text.into() });
                    }
                    if let Some(text) = delta
                        .get("reasoning_content")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                    {
                        events.push(ModelEvent::ThinkingDelta { text: text.into() });
                    }
                    if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
                        for call in calls {
                            let index = call
                                .get("index")
                                .and_then(Value::as_u64)
                                .ok_or("stream tool call missing index")?;
                            let current = self.stream_state.calls.entry(index).or_default();
                            if let Some(id) = call.get("id").and_then(Value::as_str) {
                                current.id.push_str(id);
                            }
                            if let Some(function) = call.get("function") {
                                if let Some(name) = function.get("name").and_then(Value::as_str) {
                                    current.name.push_str(name);
                                }
                                if let Some(args) =
                                    function.get("arguments").and_then(Value::as_str)
                                {
                                    current.arguments.push_str(args);
                                }
                            }
                        }
                    }
                }
                if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                    self.stream_state.reason = Some(reason.into());
                }
            }
        }
        Ok(events)
    }

    fn stream_end(&mut self) -> Result<Vec<ModelEvent>, String> {
        self.sse.finish()?;
        if !self.stream_state.done {
            return Err("Chat Completions stream ended without [DONE]".into());
        }
        let mut events = Vec::new();
        for (_, call) in std::mem::take(&mut self.stream_state.calls) {
            if call.id.is_empty() || call.name.is_empty() {
                return Err("incomplete streamed tool call".into());
            }
            events.push(ModelEvent::ToolCallStart {
                id: call.id.clone(),
                name: call.name,
            });
            events.push(ModelEvent::ToolCallArgumentsDelta {
                id: call.id.clone(),
                text: call.arguments,
            });
            events.push(ModelEvent::ToolCallEnd { id: call.id });
        }
        if let Some(usage) = self.stream_state.usage.take() {
            events.push(usage);
        }
        events.push(ModelEvent::Finished {
            reason: self.stream_state.reason.take(),
        });
        Ok(events)
    }
}

pub fn bearer_header(token: &str) -> Result<HeaderValue, String> {
    HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|e| format!("invalid bearer token: {e}"))
}
