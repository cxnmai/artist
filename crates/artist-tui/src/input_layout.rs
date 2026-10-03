//! Match glyph wrapping when sizing the editor, including tabs and wide characters.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthChar;

pub fn height(lines: &[String], width: u16, tab_length: u8) -> u16 {
    let width = usize::from(width.max(1));
    let mut rows = 0_usize;
    for line in lines {
        rows = rows.saturating_add(1);
        let mut column = 0;
        for glyph in line.graphemes(true) {
            let mut next = advance(glyph, column, tab_length);
            if column > 0 && next > width {
                rows = rows.saturating_add(1);
                column = 0;
                next = advance(glyph, column, tab_length);
            }
            column = next;
        }
    }
    rows.clamp(1, usize::from(u16::MAX)) as u16
}

fn advance(glyph: &str, mut column: usize, tab_length: u8) -> usize {
    for character in glyph.chars() {
        column += if character == '\t' {
            let tab = usize::from(tab_length.max(1));
            tab - column % tab
        } else {
            character.width().unwrap_or(0)
        };
    }
    column
}
