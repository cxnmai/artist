//! Parse tool strings into styled text, never executing terminal commands.

use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use vte::{Params, Parser, Perform};

use crate::ansi_style;

#[derive(Default)]
struct TextBuilder {
    lines: Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
    pending: String,
    style: Style,
}

impl TextBuilder {
    fn flush(&mut self) {
        if !self.pending.is_empty() {
            self.spans
                .push(Span::styled(std::mem::take(&mut self.pending), self.style));
        }
    }

    fn newline(&mut self) {
        self.flush();
        self.lines.push(Line::from(std::mem::take(&mut self.spans)));
    }
}

impl Perform for TextBuilder {
    fn print(&mut self, c: char) {
        if !c.is_control() {
            self.pending.push(c);
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => self.newline(),
            b'\t' => self.pending.push_str("    "),
            _ => {} // Ignore CR, backspace, bell, and all other control characters.
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if action == 'm' && intermediates.is_empty() && !ignore {
            self.flush();
            ansi_style::apply(&mut self.style, params);
        }
    }
    // OSC (titles, links, clipboard), DCS, cursor movement, and all other escapes are ignored.
}

pub fn parse(text: &str) -> Text<'static> {
    let mut builder = TextBuilder::default();
    Parser::new().advance(&mut builder, text.as_bytes());
    builder.newline();
    Text::from(builder.lines)
}
