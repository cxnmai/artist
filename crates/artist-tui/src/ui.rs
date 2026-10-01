use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Style;
use ratatui::widgets::Block;

use crate::app::App;
use crate::colors;
use crate::components::status;

pub fn render(frame: &mut Frame, app: &App) {
    let [body, ribbon] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());
    frame.render_widget(
        Block::default().style(Style::default().bg(colors::BACKGROUND)),
        body,
    );
    status::render(frame, ribbon, app);
}
