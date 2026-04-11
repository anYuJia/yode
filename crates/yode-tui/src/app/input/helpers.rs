/// Count lines in text by counting newline characters (\n, \r\n, or bare \r).
/// A string with no line breaks = 1 line. Trailing newline counts as an extra line.
pub fn count_lines(text: &str) -> usize {
    if text.is_empty() {
        return 1;
    }
    let mut count = 1usize;
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\r' {
            count += 1;
            if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                index += 1;
            }
        } else if bytes[index] == b'\n' {
            count += 1;
        }
        index += 1;
    }
    count
}

/// Should this pasted text be folded into a pill attachment?
pub fn should_fold_paste(text: &str) -> bool {
    count_lines(text) > 2 || text.len() > 200
}

#[cfg(test)]
mod tests {
    use super::super::InputState;

    #[test]
    fn attachment_pill_includes_line_and_char_counts() {
        let mut input = InputState::new();
        input.insert_attachment("alpha\nbeta".to_string());
        let pill = input.pill_display_text(0);
        assert!(pill.contains("2L"));
        assert!(pill.contains("10C"));
    }
}
