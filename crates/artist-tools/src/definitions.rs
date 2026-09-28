//! Model-facing schemas for the built-in tools.

use artist_core::tools::ToolDefinition;
use serde_json::json;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "read".into(),
            description: "Read UTF-8 text from a file. Output is limited to 2,000 lines or 50 KB; use offset to continue.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path"},
                    "offset": {"type": "number", "description": "Starting line (1-indexed)"},
                    "limit": {"type": "number", "description": "Maximum number of lines"}
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "edit".into(),
            description: "Replace unique, non-overlapping text blocks in one file.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "edits": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "oldText": {"type": "string"},
                                "newText": {"type": "string"}
                            },
                            "required": ["oldText", "newText"]
                        }
                    }
                },
                "required": ["path", "edits"]
            }),
        },
        ToolDefinition {
            name: "write".into(),
            description: "Create or overwrite a file, creating parent directories as needed.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "content": {"type": "string"}
                },
                "required": ["path", "content"]
            }),
        },
        ToolDefinition {
            name: "bash".into(),
            description: "Run a command in a fresh bash shell. Output is limited to the last 2,000 lines or 50 KB.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string"},
                    "timeout": {"type": "number", "description": "Optional timeout in seconds"}
                },
                "required": ["command"]
            }),
        },
    ]
}
