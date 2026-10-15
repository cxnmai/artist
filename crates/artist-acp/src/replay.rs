//! Rehydrate a frontend transcript from canonical messages, never provider replay data.
use crate::events::map_events;
use agent_client_protocol::{ContentBlock, ContentChunk, SessionUpdate, TextContent};
use artist_core::{
    context::ContextEntry,
    display::{DisplayBlock, DisplayEntry, DisplayModelEvent},
    engine::AgentEvent,
};

pub fn updates(entry: &ContextEntry) -> Vec<SessionUpdate> {
    if matches!(entry, ContextEntry::Summary { .. }) {
        return Vec::new();
    }
    let entry = DisplayEntry::from(entry);
    match &entry {
        DisplayEntry::User { text } => vec![SessionUpdate::UserMessageChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new(text.clone())),
        ))],
        DisplayEntry::Assistant { blocks } => {
            let mut events = Vec::new();
            for block in blocks {
                match block {
                    DisplayBlock::Text { text } => events.push(AgentEvent::Model {
                        event: DisplayModelEvent::TextDelta { text: text.clone() },
                    }),
                    DisplayBlock::Thinking { text } => events.push(AgentEvent::Model {
                        event: DisplayModelEvent::ThinkingDelta { text: text.clone() },
                    }),
                    DisplayBlock::ToolCall { id, name, .. } => events.push(AgentEvent::Model {
                        event: DisplayModelEvent::ToolCallStart { id: id.clone(), name: name.clone() },
                    }),
                }
            }
            events.push(AgentEvent::Assistant { entry });
            map_events(events)
        }
        DisplayEntry::ToolResult { .. } => map_events(vec![AgentEvent::ToolResult { entry }]),
    }
}
