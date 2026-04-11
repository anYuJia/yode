use super::{InputAttachment, InputState, PLACEHOLDER};
use crate::app::input::count_lines;

impl InputState {
    /// Insert a pasted text as a folded attachment at the current cursor position.
    /// Inserts a placeholder char into the text buffer so position is preserved.
    pub fn insert_attachment(&mut self, content: String) {
        let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
        let line_count = count_lines(&normalized);
        let char_count = normalized.chars().count();
        let id = self.attachments.len() + 1;
        self.attachments.push(InputAttachment {
            id,
            name: format!("Pasted text #{}", id),
            content: normalized,
            line_count,
            char_count,
        });
        self.insert_char(PLACEHOLDER);
    }

    /// Take (extract) the input, resetting state. Returns (display, payload, raw_text)
    /// Replaces placeholder chars with actual attachment content.
    pub fn take(&mut self) -> (String, String, String) {
        let raw = self.text();
        let mut attachment_iter = self.attachments.iter();
        let mut expanded = String::with_capacity(raw.len());
        for ch in raw.chars() {
            if ch == PLACEHOLDER {
                if let Some(attachment) = attachment_iter.next() {
                    expanded.push_str(&attachment.content);
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

    pub fn insert_char(&mut self, ch: char) {
        let byte_index = self.byte_index();
        self.lines[self.cursor_line].insert(byte_index, ch);
        self.cursor_col += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor_col > 0 {
            let delete_col = self.cursor_col - 1;
            let ch = self.lines[self.cursor_line].chars().nth(delete_col);
            if ch == Some(PLACEHOLDER) {
                self.remove_attachment_at_col(delete_col);
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
        let byte_index = self.byte_index();
        let rest = self.lines[self.cursor_line][byte_index..].to_string();
        self.lines[self.cursor_line].truncate(byte_index);
        self.cursor_line += 1;
        self.lines.insert(self.cursor_line, rest);
        self.cursor_col = 0;
    }

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

    /// Ctrl+K: delete from cursor to end of line.
    pub fn kill_to_end(&mut self) {
        let byte_index = self.byte_index();
        for ch in self.lines[self.cursor_line][byte_index..].chars() {
            if ch == PLACEHOLDER {
                if let Some(pos) = self.attachments.iter().position(|_| true) {
                    self.attachments.remove(pos);
                }
            }
        }
        self.lines[self.cursor_line].truncate(byte_index);
    }

    /// Ctrl+W: delete the word before cursor.
    pub fn delete_word_back(&mut self) {
        if self.cursor_col == 0 {
            return;
        }
        let line = &self.lines[self.cursor_line];
        let chars: Vec<char> = line.chars().collect();
        let mut pos = self.cursor_col;
        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        while pos > 0 && !chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        for &ch in &chars[pos..self.cursor_col] {
            if ch == PLACEHOLDER {
                if let Some(index) = self.attachments.iter().position(|_| true) {
                    self.attachments.remove(index);
                }
            }
        }
        let byte_start = self.char_to_byte(pos);
        let byte_end = self.byte_index();
        self.lines[self.cursor_line].replace_range(byte_start..byte_end, "");
        self.cursor_col = pos;
    }
}
