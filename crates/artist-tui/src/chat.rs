//! A local transcript and viewport. No backend messages are generated here.

use ratatui::widgets::{Paragraph, Wrap};
use std::ops::Range;

#[derive(Default)]
pub struct Chat {
    pub messages: Vec<String>,
    pub ranges: Vec<Range<usize>>,
    pub scroll: usize,
    pub selected: Option<usize>,
    width: u16,
    height: usize,
    total: usize,
    dirty: bool,
    follow_latest: bool,
    reveal_selected: bool,
}

impl Chat {
    pub fn push(&mut self, text: String) {
        self.messages.push(text);
        self.selected = Some(self.messages.len() - 1);
        self.dirty = true;
        self.follow_latest = true;
    }

    pub fn prepare(&mut self, width: u16, height: u16) {
        let resized = self.width != width || self.height != usize::from(height);
        if resized && !self.follow_latest && self.selected.is_some() {
            self.reveal_selected = true;
        }
        self.height = usize::from(height);
        if self.dirty || self.width != width {
            self.ranges.clear();
            let mut start = 0;
            for text in &self.messages {
                let count = Paragraph::new(text.as_str())
                    .wrap(Wrap { trim: false })
                    .line_count(width)
                    .max(1);
                self.ranges.push(start..start + count);
                start = start.saturating_add(count).saturating_add(1);
            }
            self.total = self.ranges.last().map_or(0, |range| range.end);
            self.width = width;
            self.dirty = false;
        }
        if self.follow_latest {
            self.scroll = self.max_scroll();
        } else if self.reveal_selected {
            if let Some(range) = self.selected.and_then(|index| self.ranges.get(index)) {
                if range.start < self.scroll || range.len() >= self.height {
                    self.scroll = range.start;
                } else if range.end > self.scroll.saturating_add(self.height) {
                    self.scroll = range.end.saturating_sub(self.height);
                }
            }
        }
        self.scroll = self.scroll.min(self.max_scroll());
        self.reveal_selected = false;
    }

    pub fn enter_navigation(&mut self) {
        if self.selected.is_none() {
            self.selected = self.visible_message();
        }
        self.follow_latest = false;
    }

    pub fn select(&mut self, direction: i8) {
        if self.messages.is_empty() {
            return;
        }
        let current = self
            .selected
            .or_else(|| self.visible_message())
            .unwrap_or(0);
        self.selected = Some(
            current
                .saturating_add_signed(isize::from(direction))
                .min(self.messages.len() - 1),
        );
        self.follow_latest = false;
        self.reveal_selected = true;
    }

    pub fn scroll_by(&mut self, lines: i32) {
        self.scroll = self
            .scroll
            .saturating_add_signed(lines as isize)
            .min(self.max_scroll());
        self.selected = None;
        self.follow_latest = self.scroll == self.max_scroll();
        self.reveal_selected = false;
    }

    pub fn page(&mut self, direction: i32) {
        self.scroll_by(direction.saturating_mul(self.height.saturating_sub(1).max(1) as i32));
    }

    pub fn top(&mut self) {
        self.scroll = 0;
        self.selected = if self.messages.is_empty() {
            None
        } else {
            Some(0)
        };
        self.follow_latest = false;
        self.reveal_selected = true;
    }

    pub fn bottom(&mut self) {
        self.selected = self.messages.len().checked_sub(1);
        self.follow_latest = true;
        self.reveal_selected = false;
    }

    fn max_scroll(&self) -> usize {
        self.total.saturating_sub(self.height)
    }

    fn visible_message(&self) -> Option<usize> {
        self.ranges.iter().position(|range| range.end > self.scroll)
    }
}
