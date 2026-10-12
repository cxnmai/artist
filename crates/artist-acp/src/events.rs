//! Project frontend-safe engine events into ACP streaming updates.

use agent_client_protocol::{
    ContentBlock, ContentChunk, SessionUpdate, TextContent, ToolCall, ToolCallContent,
    ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind,
};
use artist_core::display::{DisplayBlock, DisplayEntry, DisplayModelEvent};
use artist_core::engine::AgentEvent;

pub fn map_events(events: Vec<AgentEvent>) -> Vec<SessionUpdate> {
    let mut updates = Vec::new();
    for event in events {
        match event {
            AgentEvent::Model { event } => match event {
                DisplayModelEvent::TextDelta { text } => {
                    updates.push(SessionUpdate::AgentMessageChunk(text_chunk(text)));
                }
                DisplayModelEvent::ThinkingDelta { text } => {
                    updates.push(SessionUpdate::AgentThoughtChunk(text_chunk(text)));
                }
                DisplayModelEvent::ToolCallStart { id, name } => {
                    let kind = tool_kind(&name);
                    updates.push(SessionUpdate::ToolCall(
                        ToolCall::new(id, name)
                            .kind(kind)
                            .status(ToolCallStatus::Pending),
                    ));
                }
                // Argument-stream completion is not execution completion.
                _ => {}
            },
            AgentEvent::Assistant {
                entry: DisplayEntry::Assistant { blocks },
            } => {
                for block in blocks {
                    if let DisplayBlock::ToolCall {
                        id,
                        name,
                        arguments,
                    } = block
                    {
                        let fields = ToolCallUpdateFields::new()
                            .kind(tool_kind(&name))
                            .title(name)
                            .status(ToolCallStatus::InProgress)
                            .raw_input(arguments);
                        updates.push(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                            id, fields,
                        )));
                    }
                    // Text/thinking already arrived as streaming deltas. Re-emitting
                    // committed blocks would duplicate the assistant response.
                }
            }
            AgentEvent::ToolResult {
                entry:
                    DisplayEntry::ToolResult {
                        tool_call_id,
                        text,
                        is_error,
                    },
                ..
            } => {
                let status = if is_error {
                    ToolCallStatus::Failed
                } else {
                    ToolCallStatus::Completed
                };
                let fields = ToolCallUpdateFields::new()
                    .status(status)
                    .raw_output(serde_json::Value::String(text.clone()))
                    .content(vec![ToolCallContent::from(ContentBlock::Text(
                        TextContent::new(text),
                    ))]);
                updates.push(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                    tool_call_id,
                    fields,
                )));
            }
            // Terminal events are represented by the prompt response, not updates.
            // User prompts are already known to the client; metadata stays local.
            _ => {}
        }
    }
    updates
}

fn text_chunk(text: String) -> ContentChunk {
    ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
}

fn tool_kind(name: &str) -> ToolKind {
    match name {
        "read" => ToolKind::Read,
        "write" | "edit" => ToolKind::Edit,
        "grep" | "find" | "ls" => ToolKind::Search,
        "bash" => ToolKind::Execute,
        _ => ToolKind::Other,
    }
}
