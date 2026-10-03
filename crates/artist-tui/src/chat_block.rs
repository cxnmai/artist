//! One typed source for every navigable transcript block.
//! Only user messages are produced/rendered today; the other variants reserve the seam.

pub enum ChatBlock {
    UserMessage {
        text: String,
    },
    #[allow(dead_code)]
    AssistantResponse {
        text: String,
    },
    #[allow(dead_code)]
    ToolUse {
        text: String,
    },
}

impl ChatBlock {
    pub fn text(&self) -> &str {
        match self {
            Self::UserMessage { text }
            | Self::AssistantResponse { text }
            | Self::ToolUse { text } => text,
        }
    }
}
