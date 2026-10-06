use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};

use crate::app::{App, Status};
use crate::colors;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let style = Style::default()
        .bg(colors::RIBBON_BACKGROUND)
        .fg(colors::RIBBON_FOREGROUND);
    frame.render_widget(Block::default().style(style), area);
    let path = display_path(&app.status.cwd);
    let directory_width = (Line::from(path.as_ref()).width().saturating_add(2) as u16)
        .min(area.width.saturating_sub(5) / 3);
    let [mode, details, directory] = Layout::horizontal([
        Constraint::Length(5),
        Constraint::Min(0),
        Constraint::Length(directory_width),
    ])
    .areas(area);
    frame.render_widget(
        Paragraph::new(format!(
            " {}{}",
            app.mode.label(),
            if app.status.busy { "*" } else { " " }
        ))
        .style(style),
        mode,
    );
    frame.render_widget(
        Paragraph::new(details_text(&app.status, usize::from(details.width))).style(style),
        details,
    );
    let label = if Line::from(path.as_ref()).width() + 1 > usize::from(directory.width) {
        app.status
            .cwd
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or(path)
    } else {
        path
    };
    frame.render_widget(
        Paragraph::new(format!("{label} "))
            .alignment(Alignment::Right)
            .style(style),
        directory,
    );
}

fn display_path(path: &std::path::Path) -> std::borrow::Cow<'_, str> {
    if let Some(home) = std::env::var_os("HOME").filter(|home| !home.is_empty()) {
        if let Ok(relative) = path.strip_prefix(std::path::Path::new(&home)) {
            return if relative.as_os_str().is_empty() {
                "~".into()
            } else {
                format!("~/{}", relative.to_string_lossy()).into()
            };
        }
    }
    path.to_string_lossy()
}

fn details_text(status: &Status, width: usize) -> String {
    if status.loading {
        return " Loading provider…".into();
    }
    let provider = status.provider.as_deref().unwrap_or("—");
    let model = status.model.as_deref().unwrap_or("—");
    let reasoning = status
        .reasoning
        .as_deref()
        .unwrap_or(if status.model.is_some() {
            "default"
        } else {
            "—"
        });
    let context = match &status.context_usage {
        Some(usage) => format!(
            "{}/{} ({:.1}%)",
            tokens(usage.tokens),
            tokens(usage.context_window),
            usage.percent
        ),
        None => match status.context_window {
            Some(window) => format!("—/{}", tokens(window)),
            None => "—".into(),
        },
    };
    let full = format!(" {provider}/{model}  reasoning {reasoning}  ctx {context}");
    if Line::from(full.as_str()).width() <= width {
        return full;
    }
    let percent = status
        .context_usage
        .as_ref()
        .map(|usage| format!("{:.1}%", usage.percent))
        .unwrap_or_else(|| "—".into());
    let suffix = format!("  {reasoning}  ctx {percent}");
    let name_width = width.saturating_sub(Line::from(suffix.as_str()).width() + 1);
    format!(
        " {}{suffix}",
        shorten(&format!("{provider}/{model}"), name_width)
    )
}

fn shorten(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if Line::from(text).width() <= width {
        return text.into();
    }
    let mut result = String::new();
    for character in text.chars() {
        if Line::from(format!("{result}{character}…")).width() > width {
            break;
        }
        result.push(character);
    }
    result.push('…');
    result
}

fn tokens(value: u64) -> String {
    if value >= 1_000_000 {
        format!("{:.1}m", value as f64 / 1_000_000.0)
    } else if value >= 1000 {
        format!("{:.1}k", value as f64 / 1000.0)
    } else {
        value.to_string()
    }
}
