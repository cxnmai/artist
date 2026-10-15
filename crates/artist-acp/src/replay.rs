//! Rehydrate a frontend transcript from canonical messages, never provider replay data.
use crate::events::map_events;
use agent_client_protocol::{
    Client, ContentBlock, ContentChunk, Result, SessionId, SessionNotification, SessionUpdate,
    TextContent,
};
use artist_core::{
    display::{DisplayBlock, DisplayEntry, DisplayModelEvent},
    engine::AgentEvent,
};
use std::path::Path;

/// Release the database lock between pages and while sending notifications.
/// Empty pages can contain metadata only; their cursors must still be followed.
pub async fn send(directory: Option<&Path>, id: SessionId, client: &impl Client) -> Result<()> {
    let mut after = 0;
    loop {
        let page = artist_session::transcript(directory, id.0.as_ref(), after, 256)
            .await
            .map_err(crate::session::internal)?;
        for item in page.items {
            for update in updates(item.entry) {
                client
                    .session_notification(SessionNotification::new(id.clone(), update))
                    .await?;
            }
        }
        match page.next_after {
            Some(next) if next > after => after = next,
            Some(_) => {
                return Err(crate::session::internal(
                    "transcript cursor did not advance",
                ));
            }
            None => return Ok(()),
        }
    }
}

pub fn updates(entry: DisplayEntry) -> Vec<SessionUpdate> {
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
                        event: DisplayModelEvent::ToolCallStart {
                            id: id.clone(),
                            name: name.clone(),
                        },
                    }),
                }
            }
            events.push(AgentEvent::Assistant { entry });
            map_events(events)
        }
        DisplayEntry::ToolResult { .. } => map_events(vec![AgentEvent::ToolResult { entry }]),
    }
}
