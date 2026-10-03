use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Style;
use ratatui::widgets::Block;

use crate::app::App;
use crate::colors;
use crate::components::{input, status};

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let input_height = app.input.height(area.width, area.height.saturating_sub(1));
    let [body, input_area, ribbon] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(input_height),
        Constraint::Length(1),
    ])
    .areas(area);
    frame.render_widget(
        Block::default().style(Style::default().bg(colors::BACKGROUND)),
        body,
    );
    input::render(frame, input_area, &app.input);
    status::render(frame, ribbon, app);
}
