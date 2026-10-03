//! Typed tool I/O with compact/expanded presentation and JSON fallback.

use std::collections::HashMap;

use artist_core::display::{DisplayBlock, DisplayEntry};
use artist_core::tool_ui::ToolUi;

use crate::chat::Chat;
use crate::chat_block::ChatBlock;

pub struct ToolView {
    raw_input: String,
    raw_output: String,
    ui: Option<ToolUi>,
    pub(super) compact: String,
    expanded: String,
}

impl ToolView {
    fn new() -> Self {
        Self {
            raw_input: String::new(),
            raw_output: String::new(),
            ui: None,
            compact: String::new(),
            expanded: String::new(),
        }
    }

    pub fn text(&self, expanded: bool) -> &str {
        if expanded {
            &self.expanded
        } else {
            &self.compact
        }
    }

    fn refresh(&mut self) {
        let raw = join_parts(self.raw_input.clone(), self.raw_output.clone());
        match &self.ui {
            Some(ui) => {
                self.compact = ui.short.clone();
                self.expanded = join_parts(ui.short.clone(), ui.long.clone().unwrap_or(raw));
            }
            None => {
                self.compact = raw.clone();
                self.expanded = raw;
            }
        }
    }
}

fn join_parts(first: String, second: String) -> String {
    if first.is_empty() {
        second
    } else if second.is_empty() {
        first
    } else {
        format!("{first}\n{second}")
    }
}

#[derive(Default)]
pub struct ToolViews {
    indices: HashMap<String, usize>,
}

impl ToolViews {
    pub fn reset(&mut self) {
        self.indices.clear();
    }

    fn update(&mut self, chat: &mut Chat, id: &str, f: impl FnOnce(&mut ToolView)) {
        if let Some(&index) = self.indices.get(id) {
            chat.update(index, |block| {
                if let ChatBlock::ToolUse { view } = block {
                    f(view);
                    view.refresh();
                }
            });
        } else {
            let mut view = ToolView::new();
            f(&mut view);
            view.refresh();
            let index = chat.push(ChatBlock::ToolUse { view });
            self.indices.insert(id.to_owned(), index);
        }
    }

    /// Live argument fragments stay JSON until the backend commits complete typed arguments.
    pub fn input_event(&mut self, chat: &mut Chat, id: &str, json: String) {
        self.update(chat, id, |view| {
            view.raw_input = join_parts(std::mem::take(&mut view.raw_input), json);
        });
    }

    pub fn call(&mut self, chat: &mut Chat, block: DisplayBlock) {
        if let DisplayBlock::ToolCall { id, .. } = &block {
            let json = serde_json::to_string(&block).expect("tool calls serialize");
            self.update(chat, id, |view| view.raw_input = json);
        }
    }

    pub fn result(&mut self, chat: &mut Chat, entry: DisplayEntry, ui: Option<ToolUi>) {
        if let DisplayEntry::ToolResult { tool_call_id, .. } = &entry {
            let json = serde_json::to_string(&entry).expect("tool results serialize");
            self.update(chat, tool_call_id, |view| {
                view.raw_output = json;
                view.ui = ui;
            });
            self.indices.remove(tool_call_id);
        } else {
            chat.push(ChatBlock::RawJson {
                text: serde_json::to_string(&entry).expect("tool results serialize"),
            });
        }
    }
}
