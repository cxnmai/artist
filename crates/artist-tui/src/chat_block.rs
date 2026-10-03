//! One typed source for every navigable transcript block.
//! Responses and thinking are typed text blocks; tool events retain JSON presentation.

pub enum ChatBlock {
    UserMessage {
        text: String,
    },
    RawJson {
        text: String,
    },
    Notice {
        text: String,
    },
    ThinkingTrace {
        text: String,
    },
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
            | Self::RawJson { text }
            | Self::Notice { text }
            | Self::ThinkingTrace { text }
            | Self::AssistantResponse { text }
            | Self::ToolUse { text } => text,
        }
    }

    pub fn text_mut(&mut self) -> &mut String {
        match self {
            Self::UserMessage { text }
            | Self::RawJson { text }
            | Self::Notice { text }
            | Self::ThinkingTrace { text }
            | Self::AssistantResponse { text }
            | Self::ToolUse { text } => text,
        }
    }
}
