//! One typed source for every navigable transcript block.
//! Backend events temporarily use raw JSON; normalized response/tool UI is deferred.

pub enum ChatBlock {
    UserMessage {
        text: String,
    },
    RawJson {
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
            | Self::RawJson { text }
            | Self::AssistantResponse { text }
            | Self::ToolUse { text } => text,
        }
    }
}
