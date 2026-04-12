mod editing;
mod helpers;

pub use helpers::{count_lines, should_fold_paste};

/// Multi-line text input buffer with cursor state and attachments.
pub struct InputState {
    /// Multi-line input buffer
    pub lines: Vec<String>,
    /// Cursor line index (0-based)
    pub cursor_line: usize,
    /// Cursor column index (character-based, not byte)
    pub cursor_col: usize,
    /// Attachments (e.g. folded pasted text).
    /// Inline placeholder \u{FFFC} in `lines` marks where each attachment belongs.
    pub attachments: Vec<InputAttachment>,
    /// Ghost text (system suggestion shown in gray at cursor end)
    pub ghost_text: Option<String>,
}

/// Placeholder character inserted into the text buffer to mark attachment position.
pub(crate) const PLACEHOLDER: char = '\u{FFFC}';

pub struct InputAttachment {
    pub id: usize,
    pub name: String,
    pub content: String,
    pub line_count: usize,
    pub char_count: usize,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_line: 0,
            cursor_col: 0,
            attachments: Vec::new(),
            ghost_text: None,
        }
    }

    /// Whether the input is empty.
    pub fn is_empty(&self) -> bool {
        self.lines.len() == 1 && self.lines[0].is_empty() && self.attachments.is_empty()
    }

    /// Whether input has multiple lines.
    pub fn is_multiline(&self) -> bool {
        self.lines.len() > 1
    }

    /// Get the full input as a single string (joining lines with \n).
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// Get the number of input lines.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Calculate the visual line count considering line wrapping at the given terminal width.
    /// Simulates ratatui's character-level wrapping.
    pub fn visual_line_count(&self, term_width: u16) -> usize {
        if term_width == 0 {
            return self.lines.len();
        }
        let width = term_width as usize;
        let mut total_rows = 0usize;
        let mut pill_index = 0usize;
        for line in &self.lines {
            let prefix_width = 2;
            let mut col = prefix_width;
            let mut rows = 1usize;
            for ch in line.chars() {
                let char_width = if ch == PLACEHOLDER {
                    let pill_width = self.pill_width(pill_index);
                    pill_index += 1;
                    pill_width
                } else {
                    unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0)
                };
                if col + char_width > width {
                    rows += 1;
                    col = char_width;
                } else {
                    col += char_width;
                }
            }
            total_rows += rows;
        }
        total_rows
    }

    /// Get the display text for a pill/attachment at the given index.
    pub fn pill_display_text(&self, index: usize) -> String {
        if let Some(attachment) = self.attachments.get(index) {
            format!(
                "[{} · {}L · {}C]",
                attachment.name, attachment.line_count, attachment.char_count
            )
        } else {
            "[paste]".to_string()
        }
    }

    /// Get the display width of a pill/attachment at the given index.
    pub fn pill_width(&self, index: usize) -> usize {
        self.pill_display_text(index).len()
    }

    /// Calculate input area height.
    pub fn area_height(&self, terminal_height: u16) -> u16 {
        let line_count = self.line_count() as u16;
        let min_height = 1u16;
        let max_height = 5u16.min(terminal_height.saturating_sub(4));
        line_count.clamp(min_height, max_height)
    }

    /// Clear all input and reset cursor.
    pub fn clear(&mut self) {
        self.lines = vec![String::new()];
        self.cursor_line = 0;
        self.cursor_col = 0;
        self.attachments.clear();
        self.ghost_text = None;
    }

    /// Clear ghost text only.
    pub fn clear_ghost_text(&mut self) {
        self.ghost_text = None;
    }

    /// Set input to given text (single line).
    pub fn set_text(&mut self, text: &str) {
        self.lines = vec![text.to_string()];
        self.cursor_line = 0;
        self.cursor_col = text.chars().count();
        self.ghost_text = None;
    }

    /// Set ghost text (shown in gray at cursor end when cursor is at end of input).
    pub fn set_ghost_text(&mut self, text: Option<String>) {
        self.ghost_text = text;
    }

    /// Current line's char count.
    pub fn char_count(&self) -> usize {
        self.lines[self.cursor_line].chars().count()
    }

    pub(super) fn byte_index(&self) -> usize {
        self.char_to_byte(self.cursor_col)
    }

    pub(super) fn char_to_byte(&self, char_idx: usize) -> usize {
        self.lines[self.cursor_line]
            .char_indices()
            .nth(char_idx)
            .map(|(index, _)| index)
            .unwrap_or(self.lines[self.cursor_line].len())
    }

    pub(super) fn remove_attachment_at_col(&mut self, target_col: usize) {
        let mut placeholder_idx = 0;
        for line in &self.lines[..self.cursor_line] {
            placeholder_idx += line.chars().filter(|&ch| ch == PLACEHOLDER).count();
        }
        for (index, ch) in self.lines[self.cursor_line].chars().enumerate() {
            if index == target_col {
                break;
            }
            if ch == PLACEHOLDER {
                placeholder_idx += 1;
            }
        }
        if placeholder_idx < self.attachments.len() {
            self.attachments.remove(placeholder_idx);
        }
    }
}
