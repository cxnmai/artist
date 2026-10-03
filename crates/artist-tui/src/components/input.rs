use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Block;

use crate::colors;
use crate::input::InputBox;

pub fn render(frame: &mut Frame, area: Rect, input: &InputBox) {
    if area.is_empty() {
        return;
    }
    frame.render_widget(Block::default().style(InputBox::style()), area);
    // Reserve one white cell for the caret at the end of a completely full line.
    let editor_area = Rect {
        width: area.width.saturating_sub(1).max(1),
        ..area
    };
    frame.render_widget(&input.editor, editor_area);
    let cursor = input.editor.screen_cursor();
    let mut top = input.viewport_top.get();
    if cursor.row < top {
        top = cursor.row;
    } else if cursor.row >= top + usize::from(area.height) {
        top = cursor.row + 1 - usize::from(area.height);
    }
    input.viewport_top.set(top);
    if input.active && cursor.col >= usize::from(editor_area.width) {
        let caret = Rect::new(area.right() - 1, area.y + (cursor.row - top) as u16, 1, 1);
        frame.render_widget(
            Block::default().style(Style::default().bg(colors::INPUT_FOREGROUND)),
            caret,
        );
    }
}
