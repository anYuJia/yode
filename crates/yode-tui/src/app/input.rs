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
}

/// Placeholder character inserted into the text buffer to mark attachment position.
const PLACEHOLDER: char = '\u{FFFC}';

pub struct InputAttachment {
    pub id: usize,
    pub name: String,
    pub content: String,
    pub line_count: usize,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_line: 0,
            cursor_col: 0,
            attachments: Vec::new(),
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

    /// Calculate input area height.
    pub fn area_height(&self, terminal_height: u16) -> u16 {
        let line_count = self.line_count() as u16;
        let min_height = 1u16;
        let max_height = 5u16.min(terminal_height.saturating_sub(4));
        line_count.clamp(min_height, max_height)
    }

    /// Insert a pasted text as a folded attachment at the current cursor position.
    /// Inserts a placeholder char into the text buffer so position is preserved.
    pub fn insert_attachment(&mut self, content: String) {
        let line_count = count_lines(&content);
        let id = self.attachments.len() + 1;
        self.attachments.push(InputAttachment {
            id,
            name: format!("Pasted text #{}", id),
            content,
            line_count,
        });
        // Insert placeholder at cursor
        self.insert_char(PLACEHOLDER);
    }

    /// Take (extract) the input, resetting state. Returns (display, payload, raw_text)
    /// Replaces placeholder chars with actual attachment content.
    pub fn take(&mut self) -> (String, String, String) {
        let raw = self.text();

        // Build a map from placeholder index → attachment content
        let mut att_iter = self.attachments.iter();

        // Replace each PLACEHOLDER with the corresponding attachment content
        let mut expanded = String::with_capacity(raw.len());
        for ch in raw.chars() {
            if ch == PLACEHOLDER {
                if let Some(att) = att_iter.next() {
                    expanded.push_str(&att.content);
                }
            } else {
                expanded.push(ch);
            }
        }

        let display = expanded.clone();
        let payload = expanded;

        self.clear();
        (display, payload, raw)
    }

    /// Clear all input and reset cursor.
    pub fn clear(&mut self) {
        self.lines = vec![String::new()];
        self.cursor_line = 0;
        self.cursor_col = 0;
        self.attachments.clear();
    }

    /// Set input to given text (single line).
    pub fn set_text(&mut self, text: &str) {
        self.lines = vec![text.to_string()];
        self.cursor_line = 0;
        self.cursor_col = text.chars().count();
    }

    // ── Character operations ────────────────────────────────────────

    pub fn insert_char(&mut self, c: char) {
        let byte_idx = self.byte_index();
        self.lines[self.cursor_line].insert(byte_idx, c);
        self.cursor_col += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor_col > 0 {
            // Check if the char we're about to delete is a placeholder
            let del_col = self.cursor_col - 1;
            let ch = self.lines[self.cursor_line].chars().nth(del_col);
            if ch == Some(PLACEHOLDER) {
                // Remove the last attachment whose placeholder hasn't been removed yet
                self.remove_attachment_at_col(del_col);
            }
            self.cursor_col -= 1;
            let start = self.byte_index();
            let end = self.char_to_byte(self.cursor_col + 1);
            self.lines[self.cursor_line].replace_range(start..end, "");
        } else if self.cursor_line > 0 {
            let current = self.lines.remove(self.cursor_line);
            self.cursor_line -= 1;
            self.cursor_col = self.char_count();
            self.lines[self.cursor_line].push_str(&current);
        }
    }

    pub fn delete(&mut self) {
        if self.cursor_col < self.char_count() {
            let ch = self.lines[self.cursor_line].chars().nth(self.cursor_col);
            if ch == Some(PLACEHOLDER) {
                self.remove_attachment_at_col(self.cursor_col);
            }
            let start = self.byte_index();
            let end = self.char_to_byte(self.cursor_col + 1);
            self.lines[self.cursor_line].replace_range(start..end, "");
        } else if self.cursor_line < self.lines.len() - 1 {
            let next = self.lines.remove(self.cursor_line + 1);
            self.lines[self.cursor_line].push_str(&next);
        }
    }

    /// Insert a newline at cursor position (Shift+Enter / Ctrl+J).
    pub fn insert_newline(&mut self) {
        let byte_idx = self.byte_index();
        let rest = self.lines[self.cursor_line][byte_idx..].to_string();
        self.lines[self.cursor_line].truncate(byte_idx);
        self.cursor_line += 1;
        self.lines.insert(self.cursor_line, rest);
        self.cursor_col = 0;
    }

    // ── Cursor navigation ───────────────────────────────────────────

    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_line > 0 {
            self.cursor_line -= 1;
            self.cursor_col = self.char_count();
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor_col < self.char_count() {
            self.cursor_col += 1;
        } else if self.cursor_line < self.lines.len() - 1 {
            self.cursor_line += 1;
            self.cursor_col = 0;
        }
    }

    pub fn move_home(&mut self) {
        self.cursor_col = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor_col = self.char_count();
    }

    pub fn move_up(&mut self) -> bool {
        if self.cursor_line > 0 {
            self.cursor_line -= 1;
            self.cursor_col = self.cursor_col.min(self.char_count());
            true
        } else {
            false
        }
    }

    pub fn move_down(&mut self) -> bool {
        if self.cursor_line < self.lines.len() - 1 {
            self.cursor_line += 1;
            self.cursor_col = self.cursor_col.min(self.char_count());
            true
        } else {
            false
        }
    }

    // ── Line editing ────────────────────────────────────────────────

    /// Ctrl+K: delete from cursor to end of line.
    pub fn kill_to_end(&mut self) {
        let byte_idx = self.byte_index();
        // Remove any placeholder attachments in the killed range
        for ch in self.lines[self.cursor_line][byte_idx..].chars() {
            if ch == PLACEHOLDER {
                // Remove first remaining attachment
                if let Some(pos) = self.attachments.iter().position(|_| true) {
                    self.attachments.remove(pos);
                }
            }
        }
        self.lines[self.cursor_line].truncate(byte_idx);
    }

    /// Ctrl+W: delete the word before cursor.
    pub fn delete_word_back(&mut self) {
        if self.cursor_col == 0 {
            return;
        }
        let line = &self.lines[self.cursor_line];
        let chars: Vec<char> = line.chars().collect();
        let mut pos = self.cursor_col;
        // Skip trailing whitespace
        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        // Skip word characters
        while pos > 0 && !chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        // Remove any placeholder attachments in the deleted range
        for &ch in &chars[pos..self.cursor_col] {
            if ch == PLACEHOLDER {
                if let Some(p) = self.attachments.iter().position(|_| true) {
                    self.attachments.remove(p);
                }
            }
        }
        let byte_start = self.char_to_byte(pos);
        let byte_end = self.byte_index();
        self.lines[self.cursor_line].replace_range(byte_start..byte_end, "");
        self.cursor_col = pos;
    }

    // ── Internal helpers ────────────────────────────────────────────

    /// Current line's char count.
    fn char_count(&self) -> usize {
        self.lines[self.cursor_line].chars().count()
    }

    /// Convert cursor_col (char index) to byte index in current line.
    fn byte_index(&self) -> usize {
        self.char_to_byte(self.cursor_col)
    }

    /// Convert a char index to byte index in current line.
    fn char_to_byte(&self, char_idx: usize) -> usize {
        self.lines[self.cursor_line]
            .char_indices()
            .nth(char_idx)
            .map(|(i, _)| i)
            .unwrap_or(self.lines[self.cursor_line].len())
    }

    /// Remove the n-th placeholder's corresponding attachment.
    /// Counts placeholders across all lines up to the given (line, col) to find which attachment.
    fn remove_attachment_at_col(&mut self, target_col: usize) {
        let mut placeholder_idx = 0;
        // Count placeholders in lines before cursor_line
        for line in &self.lines[..self.cursor_line] {
            placeholder_idx += line.chars().filter(|&c| c == PLACEHOLDER).count();
        }
        // Count placeholders in current line up to target_col
        for (i, ch) in self.lines[self.cursor_line].chars().enumerate() {
            if i == target_col {
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

/// Count lines in text by counting newline characters (\n, \r\n, or bare \r).
/// A string with no line breaks = 1 line. Trailing newline counts as an extra line.
pub fn count_lines(text: &str) -> usize {
    if text.is_empty() {
        return 1;
    }
    let mut count = 1usize;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' {
            count += 1;
            if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                i += 1;
            }
        } else if bytes[i] == b'\n' {
            count += 1;
        }
        i += 1;
    }
    count
}

/// Should this pasted text be folded into a pill attachment?
pub fn should_fold_paste(text: &str) -> bool {
    count_lines(text) > 2 || text.len() > 200
}
