//! Assemble typed deltas and reconcile committed assistant output without duplication.

use artist_core::display::DisplayBlock;

use crate::chat::Chat;
use crate::chat_block::ChatBlock;

#[derive(Clone, Copy)]
pub enum OutputKind {
    Text,
    Thinking,
}

impl OutputKind {
    fn block(self, text: String) -> ChatBlock {
        match self {
            Self::Text => ChatBlock::AssistantResponse { text },
            Self::Thinking => ChatBlock::ThinkingTrace { text },
        }
    }

    fn matches(self, block: &ChatBlock) -> bool {
        matches!(
            (self, block),
            (Self::Text, ChatBlock::AssistantResponse { .. })
                | (Self::Thinking, ChatBlock::ThinkingTrace { .. })
        )
    }
}

#[derive(Default)]
pub struct ResponseView {
    indices: Vec<usize>,
}

impl ResponseView {
    pub fn reset(&mut self) {
        self.indices.clear();
    }

    pub fn delta(&mut self, chat: &mut Chat, kind: OutputKind, text: String) {
        if text.is_empty() {
            return;
        }
        if let Some(&index) = self.indices.last() {
            if kind.matches(&chat.blocks[index]) {
                chat.append(index, &text);
                return;
            }
        }
        self.indices.push(chat.push(kind.block(text)));
    }

    /// The backend's committed text is authoritative; tool blocks retain JSON display.
    pub fn commit(&mut self, chat: &mut Chat, blocks: Vec<DisplayBlock>) -> Vec<DisplayBlock> {
        let mut tools = Vec::new();
        let mut position = 0;
        for block in blocks {
            let output = match block {
                DisplayBlock::Text { text } if !text.is_empty() => OutputKind::Text.block(text),
                DisplayBlock::Thinking { text } if !text.is_empty() => {
                    OutputKind::Thinking.block(text)
                }
                tool @ DisplayBlock::ToolCall { .. } => {
                    tools.push(tool);
                    continue;
                }
                _ => continue,
            };
            if let Some(&index) = self.indices.get(position) {
                chat.replace(index, output);
            } else {
                chat.push(output);
            }
            position += 1;
        }
        self.reset();
        tools
    }
}
