use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
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
        let ChatBlock::UserMessage { text } = block else {
            continue; // Response/tool rendering will be added separately.
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
        let style = if navigating && chat.selected == Some(index) {
            Style::default()
                .bg(colors::USER_MESSAGE_SELECTED_BACKGROUND)
                .fg(colors::USER_MESSAGE_SELECTED_FOREGROUND)
        } else {
            Style::default()
                .bg(colors::USER_MESSAGE_BACKGROUND)
                .fg(colors::USER_MESSAGE_FOREGROUND)
        };
        // Fill the complete row width, including whitespace after the message text.
        frame.render_widget(Block::default().style(style), visible);
        frame.render_widget(
            Paragraph::new(text.as_str())
                .style(style)
                .wrap(Wrap { trim: false })
                .scroll((skipped.min(usize::from(u16::MAX)) as u16, 0)),
            visible,
        );
    }
}
