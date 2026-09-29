//! Stateless, buffered Responses API: replay native output items (including encrypted reasoning).

use super::sse::Sse;
use crate::ModelInput;
use crate::adapter::Adapter;
use crate::http::{HeaderMap, Method, Request, Response};
use artist_core::context::{AssistantBlock, ContextEntry};
use artist_core::event::ModelEvent;
use serde_json::{Value, json};

#[derive(Default)]
pub struct OpenAIResponses {
    pub name: String,
    pub endpoint: String,
    pub headers: HeaderMap,
    pub options: serde_json::Map<String, Value>,
    pub reasoning_level: Option<String>,
    pub stream: bool,
    pub(crate) sse: Sse,
    pub(crate) completed: bool,
    pub(crate) streamed_text: String,
    pub(crate) streamed_thinking: bool,
}

impl Adapter for OpenAIResponses {
    type Error = String;

    fn request(&self, context: &ModelInput<'_>) -> Result<Request, String> {
        let mut input = Vec::new();
        for entry in context.entries {
            match entry {
                ContextEntry::User { text } => input.push(json!({"role":"user","content":text})),
                ContextEntry::ToolResult {
                    tool_call_id, text, ..
                } => input.push(json!({
                    "type":"function_call_output", "call_id":tool_call_id, "output":text
                })),
                ContextEntry::Assistant { blocks } => {
                    let native: Vec<_> = blocks
                        .iter()
                        .filter_map(|b| match b {
                            AssistantBlock::ProviderData { provider, data }
                                if provider == "openai_responses" =>
                            {
                                Some(data.clone())
                            }
                            _ => None,
                        })
                        .collect();
                    if !native.is_empty() {
                        input.extend(native);
                    } else {
                        let text = blocks
                            .iter()
                            .filter_map(|b| match b {
                                AssistantBlock::Text { text } => Some(text.as_str()),
                                _ => None,
                            })
                            .collect::<String>();
                        if !text.is_empty() {
                            input.push(json!({"role":"assistant","content":text}));
                        }
                        for block in blocks {
                            if let AssistantBlock::ToolCall {
                                id,
                                name,
                                arguments,
                            } = block
                            {
                                input.push(json!({"type":"function_call","call_id":id,"name":name,"arguments":arguments.to_string()}));
                            }
                        }
                    }
                }
            }
        }
        let mut body = self.options.clone();
        body.insert("model".into(), json!(self.name));
        body.insert("input".into(), json!(input));
        body.insert("store".into(), json!(false));
        body.insert("stream".into(), json!(self.stream));
        if !context.system_prompt.is_empty() {
            body.insert("instructions".into(), json!(context.system_prompt));
        } else {
            body.remove("instructions");
        }
        // Stateless requests require encrypted reasoning items for subsequent tool turns.
        let include = body.entry("include").or_insert_with(|| json!([]));
        let items = include
            .as_array_mut()
            .ok_or("Responses include must be an array")?;
        if !items.iter().any(|v| v == "reasoning.encrypted_content") {
            items.push(json!("reasoning.encrypted_content"));
        }
        if let Some(level) = &self.reasoning_level {
            let reasoning = body.entry("reasoning").or_insert_with(|| json!({}));
            reasoning
                .as_object_mut()
                .ok_or("Responses reasoning must be an object")?
                .insert("effort".into(), json!(level));
        }
        body.remove("tools");
        if !context.tools.is_empty() {
            body.insert("tools".into(), Value::Array(context.tools.iter().map(|tool| json!({
                "type":"function","name":tool.name,"description":tool.description,"parameters":tool.parameters
            })).collect()));
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
                "Responses HTTP {}: {}",
                response.status,
                String::from_utf8_lossy(&response.body)
            ));
        }
        let body: Value =
            serde_json::from_slice(&response.body).map_err(|e| format!("Responses JSON: {e}"))?;
        if body.get("status").and_then(Value::as_str) != Some("completed") {
            return Err(format!(
                "Responses request did not complete: {}",
                body.get("status").unwrap_or(&Value::Null)
            ));
        }
        let output = body
            .get("output")
            .and_then(Value::as_array)
            .ok_or("Responses output missing")?;
        let mut events = Vec::new();
        for item in output {
            let kind = item
                .get("type")
                .and_then(Value::as_str)
                .ok_or("Responses output item missing type")?;
            events.push(ModelEvent::ProviderData {
                provider: "openai_responses".into(),
                data: item.clone(),
            });
            match kind {
                "message" => {
                    for part in item
                        .get("content")
                        .and_then(Value::as_array)
                        .ok_or("Responses message missing content")?
                    {
                        if part.get("type").and_then(Value::as_str) == Some("output_text") {
                            let text = part
                                .get("text")
                                .and_then(Value::as_str)
                                .ok_or("Responses output_text missing text")?;
                            events.push(ModelEvent::TextDelta { text: text.into() });
                        }
                    }
                }
                "reasoning" => {
                    if let Some(parts) = item.get("summary").and_then(Value::as_array) {
                        for part in parts {
                            if let Some(text) = part.get("text").and_then(Value::as_str) {
                                events.push(ModelEvent::ThinkingDelta { text: text.into() });
                            }
                        }
                    }
                }
                "function_call" => {
                    let id = item
                        .get("call_id")
                        .and_then(Value::as_str)
                        .ok_or("Responses function_call missing call_id")?
                        .to_owned();
                    let name = item
                        .get("name")
                        .and_then(Value::as_str)
                        .ok_or("Responses function_call missing name")?
                        .to_owned();
                    let args = item
                        .get("arguments")
                        .and_then(Value::as_str)
                        .ok_or("Responses function_call missing arguments")?
                        .to_owned();
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
                _ => {} // Preserve future output item types for replay.
            }
        }
        if let Some(usage) = body.get("usage") {
            if let (Some(input_tokens), Some(output_tokens)) = (
                usage.get("input_tokens").and_then(Value::as_u64),
                usage.get("output_tokens").and_then(Value::as_u64),
            ) {
                events.push(ModelEvent::Usage {
                    input_tokens,
                    output_tokens,
                    total_tokens: usage.get("total_tokens").and_then(Value::as_u64),
                    cached_input_tokens: usage
                        .pointer("/input_tokens_details/cached_tokens")
                        .and_then(Value::as_u64),
                    cache_write_input_tokens: None,
                    reasoning_output_tokens: usage
                        .pointer("/output_tokens_details/reasoning_tokens")
                        .and_then(Value::as_u64),
                    cost_usd: None,
                });
            }
        }
        events.push(ModelEvent::Finished {
            reason: Some("stop".into()),
        });
        Ok(events)
    }

    fn stream_start(&mut self, status: u16, _: &HeaderMap) -> Result<Vec<ModelEvent>, String> {
        if !(200..300).contains(&status) {
            return Err(format!("Responses HTTP {status}"));
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
                "response.output_text.delta" => {
                    let text = frame
                        .get("delta")
                        .and_then(Value::as_str)
                        .ok_or("Responses text delta missing delta")?;
                    self.streamed_text.push_str(text);
                    events.push(ModelEvent::TextDelta { text: text.into() });
                }
                "response.reasoning_summary_text.delta" => {
                    if let Some(text) = frame.get("delta").and_then(Value::as_str) {
                        self.streamed_thinking = true;
                        events.push(ModelEvent::ThinkingDelta { text: text.into() });
                    }
                }
                "response.completed" => {
                    if self.completed {
                        return Err("duplicate Responses completion".into());
                    }
                    self.completed = true;
                    let response = frame
                        .get("response")
                        .ok_or("Responses completed event missing response")?;
                    let body = serde_json::to_vec(response).map_err(|e| e.to_string())?;
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
                    if !self.streamed_text.is_empty() && final_text != self.streamed_text {
                        return Err(
                            "Responses streamed text differs from completed response".into()
                        );
                    }
                    for event in final_events {
                        match &event {
                            ModelEvent::TextDelta { .. } if !self.streamed_text.is_empty() => {}
                            ModelEvent::ThinkingDelta { .. } if self.streamed_thinking => {} // Already displayed; native item is retained.
                            _ => events.push(event),
                        }
                    }
                }
                "response.failed" | "error" => {
                    return Err(format!("Responses stream error: {frame}"));
                }
                _ => {}
            }
        }
        Ok(events)
    }

    fn stream_end(&mut self) -> Result<Vec<ModelEvent>, String> {
        self.sse.finish()?;
        if !self.completed {
            return Err("Responses stream ended without response.completed".into());
        }
        Ok(Vec::new())
    }
}
