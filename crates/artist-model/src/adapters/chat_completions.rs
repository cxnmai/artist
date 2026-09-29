//! Buffered OpenAI-style Chat Completions protocol. Endpoint/auth are supplied by the caller.

use crate::ModelInput;
use crate::adapter::Adapter;
use crate::http::{HeaderMap, HeaderValue, Method, Request, Response};
use artist_core::context::{AssistantBlock, ContextEntry};
use artist_core::event::ModelEvent;
use serde_json::{Value, json};

pub struct ChatCompletions {
    pub name: String,
    pub endpoint: String,
    pub headers: HeaderMap,
    pub options: serde_json::Map<String, Value>,
    pub reasoning_level: Option<String>,
    pub thinking_format: Option<String>,
    pub requires_reasoning_content: bool,
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
                ContextEntry::User { text } => {
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
        body.insert("stream".into(), json!(false));
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

    fn stream_chunk(&mut self, _: &[u8]) -> Result<Vec<ModelEvent>, String> {
        Err("chat completions streaming is not configured".into())
    }
}

pub fn bearer_header(token: &str) -> Result<HeaderValue, String> {
    HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|e| format!("invalid bearer token: {e}"))
}
