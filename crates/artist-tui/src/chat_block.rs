//! One typed source for every navigable transcript block.
//! Tools use optional compact/expanded presentation with JSON fallback.

use crate::tool_view::ToolView;
use ratatui::text::Text;

pub enum ChatBlock {
    UserMessage { text: String },
    RawJson { text: String },
    Notice { text: String },
    ThinkingTrace { text: String },
    AssistantResponse { text: String },
    ToolUse { view: ToolView },
}

impl ChatBlock {
    pub fn text(&self) -> &str {
        match self {
            Self::UserMessage { text }
            | Self::RawJson { text }
            | Self::Notice { text }
            | Self::ThinkingTrace { text }
            | Self::AssistantResponse { text } => text,
            Self::ToolUse { view } => view.text(false),
        }
    }

    pub fn rendered(&self, expanded: bool) -> Text<'_> {
        match self {
            Self::ToolUse { view } => view.styled(expanded),
            _ => Text::from(self.text()),
        }
    }

    pub fn text_mut(&mut self) -> &mut String {
        match self {
            Self::UserMessage { text }
            | Self::RawJson { text }
            | Self::Notice { text }
            | Self::ThinkingTrace { text }
            | Self::AssistantResponse { text } => text,
            Self::ToolUse { view } => &mut view.compact,
        }
    }
}
