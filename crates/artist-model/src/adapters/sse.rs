//! Incremental SSE framing; transport chunks need not align with lines or UTF-8.

use serde_json::Value;

#[derive(Default)]
pub struct Sse {
    pending: Vec<u8>,
    event: Option<String>,
    data: Vec<String>,
}

impl Sse {
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<(String, Value)>, String> {
        self.pending.extend_from_slice(chunk);
        if self.pending.len() > 8 * 1024 * 1024 {
            return Err("SSE frame exceeds 8 MiB".into());
        }
        let mut events = Vec::new();
        while let Some(end) = self.pending.iter().position(|&b| b == b'\n') {
            let mut line = self.pending.drain(..=end).collect::<Vec<_>>();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = String::from_utf8(line).map_err(|e| format!("SSE UTF-8: {e}"))?;
            if line.is_empty() {
                if !self.data.is_empty() {
                    let data = self.data.join("\n");
                    if data == "[DONE]" {
                        events.push(("[DONE]".into(), Value::Null));
                    } else {
                        let value: Value =
                            serde_json::from_str(&data).map_err(|e| format!("SSE JSON: {e}"))?;
                        events.push((self.event.take().unwrap_or_default(), value));
                    }
                    self.data.clear();
                }
                self.event = None;
            } else if let Some(value) = line.strip_prefix("event:") {
                self.event = Some(value.trim_start().into());
            } else if let Some(value) = line.strip_prefix("data:") {
                self.data
                    .push(value.strip_prefix(' ').unwrap_or(value).into());
            }
        }
        Ok(events)
    }

    pub fn finish(&mut self) -> Result<Vec<(String, Value)>, String> {
        if !self.pending.is_empty() || !self.data.is_empty() {
            return Err("incomplete SSE frame".into());
        }
        Ok(Vec::new())
    }
}
