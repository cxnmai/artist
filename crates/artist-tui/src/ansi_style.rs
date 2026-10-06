//! Whitelisted SGR colors and text attributes. Resets restore the widget's base style.

use ratatui::style::{Color, Modifier, Style};
use vte::Params;

use crate::colors;

pub fn apply(style: &mut Style, params: &Params) {
    let groups: Vec<&[u16]> = params.iter().collect();
    let mut index = 0;
    while let Some(group) = groups.get(index) {
        let code = group[0];
        if code == 38 || code == 48 {
            let (color, consumed) = if group.len() > 1 {
                (extended(&group[1..]), 1)
            } else {
                let mode = groups.get(index + 1).map(|g| g[0]);
                let count = match mode {
                    Some(2) => 5,
                    Some(5) => 3,
                    _ => 2,
                };
                let values: Vec<u16> = groups
                    .iter()
                    .skip(index + 1)
                    .take(count - 1)
                    .map(|g| g[0])
                    .collect();
                (extended(&values), count)
            };
            if let Some(color) = color {
                if code == 38 {
                    style.fg = Some(color);
                } else {
                    style.bg = Some(color);
                }
            }
            index += consumed;
            continue;
        }
        // Other colon subparameters aren't treated as independent SGR commands.
        match code {
            0 => *style = Style::default(),
            1 => *style = style.add_modifier(Modifier::BOLD),
            2 => *style = style.add_modifier(Modifier::DIM),
            3 => *style = style.add_modifier(Modifier::ITALIC),
            4 => *style = style.add_modifier(Modifier::UNDERLINED),
            7 => *style = style.add_modifier(Modifier::REVERSED),
            9 => *style = style.add_modifier(Modifier::CROSSED_OUT),
            22 => *style = style.remove_modifier(Modifier::BOLD | Modifier::DIM),
            23 => *style = style.remove_modifier(Modifier::ITALIC),
            24 => *style = style.remove_modifier(Modifier::UNDERLINED),
            27 => *style = style.remove_modifier(Modifier::REVERSED),
            29 => *style = style.remove_modifier(Modifier::CROSSED_OUT),
            30..=37 => style.fg = Some(colors::ansi_index((code - 30) as u8)),
            40..=47 => style.bg = Some(colors::ansi_index((code - 40) as u8)),
            90..=97 => style.fg = Some(colors::ansi_index((code - 90 + 8) as u8)),
            100..=107 => style.bg = Some(colors::ansi_index((code - 100 + 8) as u8)),
            39 => style.fg = None,
            49 => style.bg = None,
            _ => {}
        }
        index += 1;
    }
}

fn extended(values: &[u16]) -> Option<Color> {
    match values {
        [5, index] => Some(colors::ansi_index((*index).try_into().ok()?)),
        [2, r, g, b] | [2, 0, r, g, b] => Some(colors::ansi_rgb(
            (*r).try_into().ok()?,
            (*g).try_into().ok()?,
            (*b).try_into().ok()?,
        )),
        _ => None,
    }
}
