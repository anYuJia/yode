use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

use crate::app::App;
use crate::app::input::PLACEHOLDER;

pub struct WrappedInputLayout {
    pub lines: Vec<Line<'static>>,
    pub cursor_visual_y: usize,
    pub cursor_col_x: usize,
}

pub fn build_wrapped_input_layout(
    app: &App,
    term_width: usize,
    prompt_style: Style,
    text_style: Style,
    pill_style: Style,
    ghost_style: Style,
) -> WrappedInputLayout {
    let mut visual_lines: Vec<Line<'static>> = Vec::new();
    let mut attachment_index = 0usize;
    let mut cursor_visual_y = 0usize;
    let mut cursor_col_x = 0usize;

    for (line_index, logical_line) in app.input.lines.iter().enumerate() {
        let prefix_str = if line_index == 0 { "❯ " } else { "  " };
        let prefix_width = 2usize;

        let mut items: Vec<(String, Style, usize)> = vec![(prefix_str.to_string(), prompt_style, prefix_width)];
        let mut buffer = String::new();
        for ch in logical_line.chars() {
            if ch == PLACEHOLDER {
                if !buffer.is_empty() {
                    let width = unicode_width::UnicodeWidthStr::width(buffer.as_str());
                    items.push((buffer.clone(), text_style, width));
                    buffer.clear();
                }
                let pill_text = app.input.pill_display_text(attachment_index);
                let width = pill_text.len();
                items.push((pill_text, pill_style, width));
                attachment_index += 1;
            } else {
                buffer.push(ch);
            }
        }
        if !buffer.is_empty() {
            let width = unicode_width::UnicodeWidthStr::width(buffer.as_str());
            items.push((buffer, text_style, width));
        }

        let is_cursor_line = line_index == app.input.cursor_line;
        let visual_y_before = visual_lines.len();
        let mut row_spans: Vec<Span<'static>> = Vec::new();
        let mut col = 0usize;

        for (text, style, item_width) in &items {
            if term_width > 0 && col + item_width > term_width {
                wrap_item_into_lines(
                    text,
                    *style,
                    term_width,
                    &mut col,
                    &mut row_spans,
                    &mut visual_lines,
                );
            } else {
                row_spans.push(Span::styled(text.clone(), *style));
                col += item_width;
            }
        }

        let is_last_line = line_index == app.input.lines.len() - 1;
        let cursor_at_end = app.input.cursor_col == app.input.char_count();
        if is_last_line && cursor_at_end && app.input.cursor_line == app.input.lines.len() - 1 {
            if let Some(ghost) = &app.input.ghost_text {
                row_spans.push(Span::styled(ghost.clone(), ghost_style));
            }
        }

        if !row_spans.is_empty() {
            visual_lines.push(Line::from(row_spans));
        }

        if is_cursor_line {
            let mut pill_scan = app
                .input
                .lines
                .iter()
                .take(line_index)
                .map(|line| line.chars().filter(|&c| c == PLACEHOLDER).count())
                .sum::<usize>();
            let mut cursor_col = prefix_width;
            let mut cursor_row = 0usize;
            for ch in logical_line.chars().take(app.input.cursor_col) {
                let char_width = if ch == PLACEHOLDER {
                    let width = app.input.pill_width(pill_scan);
                    pill_scan += 1;
                    width
                } else {
                    UnicodeWidthChar::width(ch).unwrap_or(0)
                };
                if term_width > 0 && cursor_col + char_width > term_width {
                    cursor_row += 1;
                    cursor_col = char_width;
                } else {
                    cursor_col += char_width;
                }
            }
            cursor_visual_y = visual_y_before + cursor_row;
            cursor_col_x = cursor_col;
        }
    }

    WrappedInputLayout {
        lines: visual_lines,
        cursor_visual_y,
        cursor_col_x,
    }
}

fn wrap_item_into_lines(
    text: &str,
    style: Style,
    term_width: usize,
    col: &mut usize,
    row_spans: &mut Vec<Span<'static>>,
    visual_lines: &mut Vec<Line<'static>>,
) {
    let mut remaining = text;
    while !remaining.is_empty() {
        let mut chunk = String::new();
        let mut chunk_width = 0usize;
        let mut chars = remaining.char_indices().peekable();
        while let Some(&(_byte_index, ch)) = chars.peek() {
            let char_width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if term_width > 0
                && *col + chunk_width + char_width > term_width
                && (*col + chunk_width) > 0
            {
                break;
            }
            if term_width > 0 && *col == 0 && chunk_width + char_width > term_width && chunk_width > 0 {
                break;
            }
            chunk.push(ch);
            chunk_width += char_width;
            chars.next();
        }
        if !chunk.is_empty() {
            row_spans.push(Span::styled(chunk.clone(), style));
            *col += chunk_width;
            remaining = &remaining[chunk.len()..];
        }
        if !remaining.is_empty() {
            visual_lines.push(Line::from(std::mem::take(row_spans)));
            *col = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::style::Style;

    use super::build_wrapped_input_layout;
    use crate::app::App;

    #[test]
    fn cursor_position_tracks_wrapped_lines() {
        let mut app = App::new(
            "model".to_string(),
            "session".to_string(),
            ".".to_string(),
            "provider".to_string(),
            vec![],
            std::collections::HashMap::new(),
            std::sync::Arc::new(yode_llm::registry::ProviderRegistry::new()),
            std::sync::Arc::new(yode_tools::registry::ToolRegistry::new()),
        );
        app.input.set_text("abcdefghij");
        app.input.cursor_col = 8;

        let layout = build_wrapped_input_layout(
            &app,
            6,
            Style::default(),
            Style::default(),
            Style::default(),
            Style::default(),
        );

        assert!(layout.cursor_visual_y >= 1);
        assert!(layout.cursor_col_x <= 6);
    }
}
