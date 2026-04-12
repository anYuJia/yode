use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::UnicodeWidthChar;

use crate::app::App;
use crate::ui::palette::{GHOST_COLOR, HINT_COLOR, PROMPT_COLOR, PROMPT_DIM, TEXT_COLOR};

use super::completions::{render_file_popup, render_history_search};

pub fn render_input(frame: &mut Frame, area: Rect, app: &App) {
    if area.height == 0 {
        return;
    }

    if app.history.is_searching() {
        render_history_search(frame, area, app);
        return;
    }

    let prompt_color = if app.is_thinking {
        PROMPT_DIM
    } else {
        PROMPT_COLOR
    };
    let prompt = Span::styled(
        "❯ ",
        Style::default()
            .fg(prompt_color)
            .add_modifier(Modifier::BOLD),
    );

    let is_empty = app.input.is_empty() && app.input.attachments.is_empty();

    if is_empty && !app.is_thinking {
        let paragraph = if let Some(ghost) = &app.input.ghost_text {
            Paragraph::new(Line::from(vec![
                prompt,
                Span::styled(ghost.clone(), Style::default().fg(GHOST_COLOR)),
            ]))
        } else {
            Paragraph::new(Line::from(vec![
                prompt,
                Span::styled("Ask anything…", Style::default().fg(HINT_COLOR)),
            ]))
        };
        frame.render_widget(paragraph, area);
    } else if app.is_thinking && is_empty {
        let paragraph = Paragraph::new(Line::from(vec![
            prompt,
            Span::styled("Ask anything…", Style::default().fg(HINT_COLOR)),
        ]));
        frame.render_widget(paragraph, area);
    } else {
        render_wrapped_input(frame, area, app, prompt_color);
    }

    if app.file_completion.is_active() {
        render_file_popup(frame, area, app);
    }
}

fn render_wrapped_input(frame: &mut Frame, area: Rect, app: &App, prompt_color: Color) {
    let term_width = area.width as usize;
    let max_visible = area.height as usize;
    let mut visual_lines: Vec<Line<'static>> = Vec::new();
    let mut attachment_index = 0usize;
    let mut cursor_visual_y = 0usize;
    let mut cursor_col_x = 0usize;

    for (line_index, logical_line) in app.input.lines.iter().enumerate() {
        let prefix_str = if line_index == 0 { "❯ " } else { "  " };
        let prefix_width = 2usize;

        let mut items: Vec<(String, Style, usize)> = Vec::new();
        items.push((
            prefix_str.to_string(),
            Style::default()
                .fg(prompt_color)
                .add_modifier(Modifier::BOLD),
            prefix_width,
        ));

        let mut buffer = String::new();
        for ch in logical_line.chars() {
            if ch == '\u{FFFC}' {
                if !buffer.is_empty() {
                    let width = unicode_width::UnicodeWidthStr::width(buffer.as_str());
                    items.push((buffer.clone(), Style::default().fg(TEXT_COLOR), width));
                    buffer.clear();
                }
                let pill_text = app.input.pill_display_text(attachment_index);
                let width = pill_text.len();
                items.push((
                    pill_text,
                    Style::default()
                        .fg(Color::LightCyan)
                        .add_modifier(Modifier::BOLD),
                    width,
                ));
                attachment_index += 1;
            } else {
                buffer.push(ch);
            }
        }
        if !buffer.is_empty() {
            let width = unicode_width::UnicodeWidthStr::width(buffer.as_str());
            items.push((buffer, Style::default().fg(TEXT_COLOR), width));
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
                row_spans.push(Span::styled(
                    ghost.clone(),
                    Style::default().fg(GHOST_COLOR),
                ));
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
                .map(|line| line.chars().filter(|&c| c == '\u{FFFC}').count())
                .sum::<usize>();
            let mut cursor_col = prefix_width;
            let mut cursor_row = 0usize;
            for ch in logical_line.chars().take(app.input.cursor_col) {
                let char_width = if ch == '\u{FFFC}' {
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

    let total = visual_lines.len();
    let skip = total.saturating_sub(max_visible);
    let visible: Vec<Line> = visual_lines
        .into_iter()
        .skip(skip)
        .take(max_visible)
        .collect();
    frame.render_widget(Paragraph::new(visible), area);

    cursor_visual_y = cursor_visual_y.saturating_sub(skip);
    if !app.is_thinking && app.pending_confirmation.is_none() {
        let cursor_y = area.y + cursor_visual_y as u16;
        let max_y = area.y + area.height.saturating_sub(1);
        frame.set_cursor_position((area.x + cursor_col_x as u16, cursor_y.min(max_y)));
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
            if term_width > 0
                && *col == 0
                && chunk_width + char_width > term_width
                && chunk_width > 0
            {
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
