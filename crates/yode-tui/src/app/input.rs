/// Input editing state and cursor management.
///
/// Encapsulates multi-line text editing, cursor navigation,
/// and line manipulation operations.

/// Multi-line text input buffer with cursor state.
pub struct InputState {
    /// Multi-line input buffer
    pub lines: Vec<String>,
    /// Cursor line index (0-based)
    pub cursor_line: usize,
    /// Cursor column index (character-based, not byte)
    pub cursor_col: usize,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_line: 0,
            cursor_col: 0,
        }
    }

    /// Whether the input is empty.
    pub fn is_empty(&self) -> bool {
        self.lines.len() == 1 && self.lines[0].is_empty()
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
        let min_height = 2u16; // separator + input line
        let max_height = (terminal_height / 3).max(min_height);
        (line_count + 1).clamp(min_height, max_height)
    }

    /// Take (extract) the input, resetting state.
    pub fn take(&mut self) -> String {
        let text = self.text();
        self.clear();
        text
    }

    /// Clear all input and reset cursor.
    pub fn clear(&mut self) {
        self.lines = vec![String::new()];
        self.cursor_line = 0;
        self.cursor_col = 0;
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
            self.cursor_col -= 1;
            let byte_idx = self.byte_index();
            self.lines[self.cursor_line].remove(byte_idx);
        } else if self.cursor_line > 0 {
            let current = self.lines.remove(self.cursor_line);
            self.cursor_line -= 1;
            self.cursor_col = self.char_count();
            self.lines[self.cursor_line].push_str(&current);
        }
    }

    pub fn delete(&mut self) {
        if self.cursor_col < self.char_count() {
            let byte_idx = self.byte_index();
            self.lines[self.cursor_line].remove(byte_idx);
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
}
