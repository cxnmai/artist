use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::Block;

use crate::app::App;
use crate::colors;
use crate::components::{chat, input, status};
use crate::mode::Mode;

pub struct Areas {
    pub chat: Rect,
    input: Rect,
    ribbon: Rect,
}

pub fn areas(area: Rect, app: &App) -> Areas {
    let input_height = app.input.height(area.width, area.height.saturating_sub(1));
    let [chat, input, ribbon] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(input_height),
        Constraint::Length(1),
    ])
    .areas(area);
    Areas {
        chat,
        input,
        ribbon,
    }
}

pub fn render(frame: &mut Frame, app: &App, areas: Areas) {
    frame.render_widget(
        Block::default().style(Style::default().bg(colors::BACKGROUND)),
        frame.area(),
    );
    // Chat is clipped to its viewport; the editor and ribbon never participate in scrolling.
    chat::render(
        frame,
        areas.chat,
        &app.chat,
        matches!(app.mode, Mode::Navigation),
    );
    input::render(frame, areas.input, &app.input);
    status::render(frame, areas.ribbon, app);
}
