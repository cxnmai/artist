use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Paragraph, Wrap};

use crate::chat::Chat;
use crate::chat_block::ChatBlock;
use crate::colors;

pub fn render(frame: &mut Frame, area: Rect, chat: &Chat, navigating: bool) {
    if area.is_empty() {
        return;
    }
    let bottom = chat.scroll.saturating_add(usize::from(area.height));
    for (index, (block, range)) in chat.blocks.iter().zip(&chat.ranges).enumerate() {
        let text = block.text_for(navigating && chat.selected == Some(index));
        let (foreground, background) = match block {
            ChatBlock::UserMessage { .. } => (
                colors::USER_MESSAGE_FOREGROUND,
                colors::USER_MESSAGE_BACKGROUND,
            ),
            ChatBlock::RawJson { .. } => (colors::RAW_JSON_FOREGROUND, colors::RAW_JSON_BACKGROUND),
            ChatBlock::AssistantResponse { .. }
            | ChatBlock::Notice { .. }
            | ChatBlock::ToolUse { .. } => (colors::OUTPUT_FOREGROUND, colors::BACKGROUND),
            ChatBlock::ThinkingTrace { .. } => (colors::THINKING_FOREGROUND, colors::BACKGROUND),
        };
        if range.end <= chat.scroll || range.start >= bottom {
            continue;
        }
        let skipped = chat.scroll.saturating_sub(range.start);
        let top = range.start.saturating_sub(chat.scroll);
        let height = range
            .len()
            .saturating_sub(skipped)
            .min(usize::from(area.height) - top);
        let visible = Rect::new(area.x, area.y + top as u16, area.width, height as u16);
        let mut style = if navigating && chat.selected == Some(index) {
            Style::default().bg(foreground).fg(background)
        } else {
            Style::default().bg(background).fg(foreground)
        };
        if matches!(block, ChatBlock::ThinkingTrace { .. }) {
            style = style.add_modifier(Modifier::ITALIC);
        }
        // Fill the complete row width, including whitespace after the message text.
        frame.render_widget(Block::default().style(style), visible);
        frame.render_widget(
            Paragraph::new(text)
                .style(style)
                .wrap(Wrap { trim: false })
                .scroll((skipped.min(usize::from(u16::MAX)) as u16, 0)),
            visible,
        );
    }
}
