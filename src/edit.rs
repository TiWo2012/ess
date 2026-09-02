//! A minimal single-line text input widget.
//!
//! ratatui deliberately ships no text input widget (it only renders), so this
//! is the small, classic pattern: a `Paragraph` plus a real terminal cursor.

use ratatui::{
    layout::{Position, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Paragraph},
    Frame,
};

pub struct EditValue {
    value: String,
    /// Cursor position as a char index (not a byte index), so multibyte
    /// UTF-8 input is handled correctly.
    cursor: usize,
    /// When set, the value renders as `*` (for password fields).
    masked: bool,
    /// When set, only ASCII digits can be inserted (e.g. port fields).
    numeric: bool,
}

impl EditValue {
    pub fn new(masked: bool) -> Self {
        Self {
            value: String::new(),
            cursor: 0,
            masked,
            numeric: false,
        }
    }

    /// Build from an existing value (used to prefill the edit form).
    pub fn from(value: &str, masked: bool) -> Self {
        Self {
            value: value.to_string(),
            cursor: value.chars().count(),
            masked,
            numeric: false,
        }
    }

    pub fn numeric(mut self) -> Self {
        self.numeric = true;
        self
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    pub fn insert_char(&mut self, c: char) {
        if self.numeric && !c.is_ascii_digit() {
            return;
        }
        self.value.insert(self.byte_index(), c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let mut chars: Vec<char> = self.value.chars().collect();
        chars.remove(self.cursor - 1);
        self.value = chars.into_iter().collect();
        self.cursor -= 1;
    }

    pub fn delete(&mut self) {
        let mut chars: Vec<char> = self.value.chars().collect();
        if self.cursor < chars.len() {
            chars.remove(self.cursor);
            self.value = chars.into_iter().collect();
        }
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.value.chars().count() {
            self.cursor += 1;
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.value.chars().count();
    }

    /// Byte index matching `self.cursor` (chars).
    fn byte_index(&self) -> usize {
        self.value
            .char_indices()
            .nth(self.cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.value.len())
    }

    /// Render the field inside `area` using `block` (supply a styled block to
    /// indicate focus). While focused, the terminal cursor is placed inside
    /// the field.
    pub fn render(&self, frame: &mut Frame, area: Rect, block: Block, focused: bool) {
        frame.render_widget(
            Paragraph::new(self.visible_text(area.width)).block(block),
            area,
        );

        if focused {
            let (_, cursor_col) = self.window(area.width.saturating_sub(2) as usize);
            frame.set_cursor_position(Position {
                x: area.x + 1 + cursor_col as u16,
                y: area.y + 1,
            });
        }
    }

    /// What the user sees: masked characters or the real value, truncated to
    /// `width` columns.
    fn visible_text(&self, width: u16) -> String {
        let inner = width.saturating_sub(2) as usize;
        let chars = self.display_chars();
        let (start, _) = self.window(inner);
        let end = (start + inner).min(chars.len());
        chars[start..end].iter().collect()
    }

    /// `*` for every char when masked, otherwise the value as chars.
    fn display_chars(&self) -> Vec<char> {
        if self.masked {
            vec!['*'; self.value.chars().count()]
        } else {
            self.value.chars().collect()
        }
    }

    /// Returns `(first visible char index, cursor column within the window)`
    /// so long lines scroll to keep the cursor on screen.
    fn window(&self, inner_width: usize) -> (usize, usize) {
        let chars = self.display_chars();
        let cursor = self.cursor.min(chars.len());
        let prefix: String = chars[..cursor].iter().collect();
        let cursor_col = Line::from(prefix).width();

        if cursor_col < inner_width {
            (0, cursor_col)
        } else {
            // Walk back from the cursor until the window is full.
            let mut back = 0usize;
            let mut col = 0usize;
            for c in chars[..cursor].iter().rev() {
                let w = Line::from(String::from(*c)).width();
                if col + w > inner_width.saturating_sub(2) {
                    break;
                }
                col += w;
                back += 1;
            }
            (cursor - back, col)
        }
    }
}

/// Convenience: focused border style.
pub fn focus_border(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}
