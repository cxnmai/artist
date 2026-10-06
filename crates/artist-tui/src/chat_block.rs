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

    /// Horizontal padding for tool bands, preserving at least one text column.
    pub fn padding(&self, width: u16) -> u16 {
        if matches!(self, Self::ToolUse { .. }) {
            2.min(width.saturating_sub(1) / 2)
        } else {
            0
        }
    }

    pub fn content_width(&self, width: u16) -> u16 {
        width.saturating_sub(2 * self.padding(width))
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
