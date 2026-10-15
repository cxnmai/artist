use crate::{Settings, replay, storage::StoredRecord};
use artist_core::{
    context::{AssistantBlock, ContextEntry, Conversation},
    journal::SessionRecord,
};
use std::path::PathBuf;

pub(crate) struct ResumePoint {
    pub sequence: i64,
    pub settings: Settings,
    pub pending: Vec<String>,
}

#[derive(Default)]
pub(crate) struct ReplayState {
    pub header: Option<(PathBuf, Settings)>,
    pub conversation: Conversation,
    pub pending: Vec<String>,
    pub after: i64,
    pub point: Option<ResumePoint>,
}

impl ReplayState {
    pub fn push(&mut self, sequence: i64, record: StoredRecord) -> Result<(), String> {
        match record {
            StoredRecord::Header {
                cwd,
                settings,
                system_prompt,
            } if self.header.is_none() && sequence == 1 => {
                self.conversation.system_prompt = system_prompt;
                self.header = Some((cwd, settings));
            }
            StoredRecord::Header { .. } => return Err("invalid duplicate session header".into()),
            StoredRecord::Settings { settings } => {
                let (_, previous) = self
                    .header
                    .as_mut()
                    .ok_or("settings before session header")?;
                replay::change_settings(&mut self.conversation, previous, &settings);
                *previous = settings;
            }
            StoredRecord::Journal { record } => {
                let (_, settings) = self.header.as_ref().ok_or("record before session header")?;
                if let SessionRecord::Message { entry } = &record {
                    match entry {
                        ContextEntry::Assistant { blocks } => {
                            for block in blocks {
                                if let AssistantBlock::ToolCall { id, .. } = block {
                                    self.pending.push(id.clone());
                                }
                            }
                        }
                        ContextEntry::ToolResult { tool_call_id, .. } => {
                            self.pending.retain(|id| id != tool_call_id);
                        }
                        _ => {}
                    }
                }
                // Cache boundary state, not the eventual end-of-batch settings/pending.
                if matches!(record, SessionRecord::Compaction { .. }) {
                    self.point = Some(ResumePoint {
                        sequence,
                        settings: settings.clone(),
                        pending: self.pending.clone(),
                    });
                }
                replay::apply(&mut self.conversation, record);
            }
        }
        self.after = sequence;
        Ok(())
    }

    pub fn recovery(&self) -> Vec<StoredRecord> {
        self.pending.iter().map(|tool_call_id| StoredRecord::Journal {
            record: SessionRecord::Message {
                entry: ContextEntry::ToolResult {
                    tool_call_id: tool_call_id.clone(),
                    text: "Tool execution interrupted; outcome unknown. The tool was not re-executed."
                        .into(),
                    is_error: true,
                },
            },
        }).collect()
    }
}
