use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthChar;

use crate::app::App;

const PROMPT_COLOR: Color = Color::Rgb(80, 200, 120);
const PROMPT_DIM: Color = Color::Rgb(60, 60, 65);
const TEXT_COLOR: Color = Color::Rgb(220, 220, 225);
const HINT_COLOR: Color = Color::Rgb(70, 70, 80);

pub fn render_input(frame: &mut Frame, area: Rect, app: &App) {
    if area.height == 0 { return; }

    // History search mode
    if app.history.search_mode {
        render_history_search(frame, area, app);
        return;
    }

    let prompt_color = if app.is_thinking { PROMPT_DIM } else { PROMPT_COLOR };
    let prompt = Span::styled("❯ ", Style::default().fg(prompt_color).add_modifier(Modifier::BOLD));

    let is_empty = app.input.is_empty() && app.input.attachments.is_empty();

    if is_empty && !app.is_thinking {
        let paragraph = Paragraph::new(Line::from(vec![
            prompt,
            Span::styled("Ask anything…", Style::default().fg(HINT_COLOR)),
        ]));
        frame.render_widget(paragraph, area);
    } else if app.is_thinking && is_empty {
        // Show spinner in prompt while thinking
        let spinner = app.spinner_char();
        let elapsed_str = app.thinking_elapsed_str();
        let queue_info = if !app.pending_inputs.is_empty() {
            format!(" ({} queued)", app.pending_inputs.len())
        } else {
            String::new()
        };
        let paragraph = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{} ", spinner),
                Style::default().fg(Color::Rgb(230, 190, 60)),
            ),
            Span::styled(
                format!("Working… {}{}", elapsed_str, queue_info),
                Style::default().fg(HINT_COLOR),
            ),
        ]));
        frame.render_widget(paragraph, area);
    } else {
        // Render text input with manual character-level wrapping.
        // We can't use ratatui's Wrap (word-level), so we split into visual lines ourselves.
        let term_w = area.width as usize;
        let max_visible = area.height as usize;

        let mut visual_lines: Vec<Line> = Vec::new();
        let mut att_idx = 0usize;

        for (i, logical_line) in app.input.lines.iter().enumerate() {
            let prefix_str = if i == 0 { "❯ " } else { "  " };
            let prefix_w = 2usize;

            // Build (char, display_str, style, width) for each character
            let mut items: Vec<(String, Style, usize)> = Vec::new();
            items.push((prefix_str.to_string(), Style::default().fg(prompt_color).add_modifier(Modifier::BOLD), prefix_w));

            let mut buf = String::new();
            for ch in logical_line.chars() {
                if ch == '\u{FFFC}' {
                    if !buf.is_empty() {
                        let w = unicode_width::UnicodeWidthStr::width(buf.as_str());
                        items.push((buf.clone(), Style::default().fg(TEXT_COLOR), w));
                        buf.clear();
                    }
                    let pill_text = if let Some(att) = app.input.attachments.get(att_idx) {
                        format!("[{} +{} lines]", att.name, att.line_count)
                    } else {
                        "[paste]".to_string()
                    };
                    let w = pill_text.len();
                    items.push((pill_text, Style::default().fg(Color::Rgb(150, 200, 255)).add_modifier(Modifier::BOLD), w));
                    att_idx += 1;
                } else {
                    buf.push(ch);
                }
            }
            if !buf.is_empty() {
                let w = unicode_width::UnicodeWidthStr::width(buf.as_str());
                items.push((buf, Style::default().fg(TEXT_COLOR), w));
            }

            // Now split items into visual lines at term_w
            let mut row_spans: Vec<Span> = Vec::new();
            let mut col = 0usize;

            for (text, style, item_w) in &items {
                if term_w > 0 && col + item_w > term_w && col > 0 {
                    // This item doesn't fit; need to split character by character
                    let mut remaining = text.as_str();
                    while !remaining.is_empty() {
                        let mut chunk = String::new();
                        let mut chunk_w = 0usize;
                        let mut chars = remaining.char_indices().peekable();
                        while let Some(&(byte_i, ch)) = chars.peek() {
                            let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
                            if term_w > 0 && col + chunk_w + cw > term_w && (col + chunk_w) > 0 {
                                break;
                            }
                            chunk.push(ch);
                            chunk_w += cw;
                            chars.next();
                        }
                        if !chunk.is_empty() {
                            row_spans.push(Span::styled(chunk.clone(), *style));
                            col += chunk_w;
                            remaining = &remaining[chunk.len()..];
                        }
                        if !remaining.is_empty() {
                            // Push current row and start new visual line
                            visual_lines.push(Line::from(std::mem::take(&mut row_spans)));
                            col = 0;
                        }
                    }
                } else if term_w > 0 && col + item_w > term_w && col == 0 {
                    // Item wider than term_w starting at col 0; split char by char
                    let mut remaining = text.as_str();
                    while !remaining.is_empty() {
                        let mut chunk = String::new();
                        let mut chunk_w = 0usize;
                        let mut chars = remaining.char_indices().peekable();
                        while let Some(&(_byte_i, ch)) = chars.peek() {
                            let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
                            if chunk_w + cw > term_w && chunk_w > 0 {
                                break;
                            }
                            chunk.push(ch);
                            chunk_w += cw;
                            chars.next();
                        }
                        if !chunk.is_empty() {
                            row_spans.push(Span::styled(chunk.clone(), *style));
                            col = chunk_w;
                            remaining = &remaining[chunk.len()..];
                        }
                        if !remaining.is_empty() {
                            visual_lines.push(Line::from(std::mem::take(&mut row_spans)));
                            col = 0;
                        }
                    }
                } else {
                    row_spans.push(Span::styled(text.clone(), *style));
                    col += item_w;
                }
            }
            if !row_spans.is_empty() {
                visual_lines.push(Line::from(row_spans));
            }
        }

        // Take only the visible portion
        let skip = visual_lines.len().saturating_sub(max_visible);
        let visible: Vec<Line> = visual_lines.into_iter().skip(skip).take(max_visible).collect();
        frame.render_widget(Paragraph::new(visible), area);
    }

    // Cursor
    if !app.is_thinking && app.pending_confirmation.is_none() {
        let term_w = area.width as usize;
        let prefix_w = 2usize;

        // Simulate wrapping for lines before cursor_line to get visual_y
        let mut visual_y = 0usize;
        let mut pill_idx = 0usize;
        for line in app.input.lines.iter().take(app.input.cursor_line) {
            let mut col = prefix_w;
            let mut rows = 1usize;
            for ch in line.chars() {
                let cw = if ch == '\u{FFFC}' {
                    let w = app.input.attachments.get(pill_idx)
                        .map(|a| format!("[{} +{} lines]", a.name, a.line_count).len())
                        .unwrap_or(6);
                    pill_idx += 1;
                    w
                } else {
                    UnicodeWidthChar::width(ch).unwrap_or(0)
                };
                if term_w > 0 && col + cw > term_w {
                    rows += 1;
                    col = cw;
                } else {
                    col += cw;
                }
            }
            visual_y += rows;
        }

        // Simulate wrapping for cursor line up to cursor_col
        let mut col = prefix_w;
        let mut wrap_row = 0usize;
        for ch in app.input.lines[app.input.cursor_line].chars().take(app.input.cursor_col) {
            let cw = if ch == '\u{FFFC}' {
                let w = app.input.attachments.get(pill_idx)
                    .map(|a| format!("[{} +{} lines]", a.name, a.line_count).len())
                    .unwrap_or(6);
                pill_idx += 1;
                w
            } else {
                UnicodeWidthChar::width(ch).unwrap_or(0)
            };
            if term_w > 0 && col + cw > term_w {
                wrap_row += 1;
                col = cw;
            } else {
                col += cw;
            }
        }

        // Clamp cursor within input area
        let cursor_y = area.y + (visual_y + wrap_row) as u16;
        let max_y = area.y + area.height.saturating_sub(1);
        frame.set_cursor_position((
            area.x + col as u16,
            cursor_y.min(max_y),
        ));
    }

    // Completion popups
    if app.file_completion.is_active() {
        render_file_popup(frame, area, app);
    } else if app.cmd_completion.is_active() {
        render_command_popup(frame, area, app);
    }
}

/// Render attachment pill tags in a separate area below input.
pub fn render_attachments(frame: &mut Frame, area: Rect, app: &App) {
    let mut spans = Vec::new();
    for att in &app.input.attachments {
        spans.push(Span::styled(
            format!("[{} +{} lines] ", att.name, att.line_count),
            Style::default().fg(Color::Rgb(150, 200, 255)).add_modifier(Modifier::BOLD),
        ));
    }
    if !spans.is_empty() {
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}

fn render_command_popup(frame: &mut Frame, area: Rect, app: &App) {
    let max_show = 8usize;
    let viewport_top = frame.area().top();
    // Space above input is (area.y - viewport_top)
    let max_avail = area.y.saturating_sub(viewport_top);
    
    let count = app.cmd_completion.candidates.len().min(max_show);
    let popup_height = (count as u16).min(max_avail); 
    if popup_height == 0 { return; }

    let popup_y = area.y.saturating_sub(popup_height);
    let popup_width = 48u16.min(area.width.saturating_sub(area.x + 2));
    let popup_area = Rect::new(area.x + 2, popup_y, popup_width, popup_height);

    frame.render_widget(Clear, popup_area);

    let selected = app.cmd_completion.selected.unwrap_or(0);
    let bg = Color::Rgb(35, 35, 40);
    let sel_bg = Color::Rgb(60, 130, 180);

    let items: Vec<Line> = app.cmd_completion.candidates
        .iter()
        .take(max_show)
        .enumerate()
        .map(|(i, (cmd, desc))| {
            if i == selected {
                Line::from(vec![
                    Span::styled(format!(" {:<12}", cmd), Style::default().fg(Color::White).bg(sel_bg).add_modifier(Modifier::BOLD)),
                    Span::styled(format!(" {} ", desc), Style::default().fg(Color::Rgb(200, 200, 210)).bg(sel_bg)),
                ])
            } else {
                Line::from(vec![
                    Span::styled(format!(" {:<12}", cmd), Style::default().fg(Color::Rgb(180, 180, 190)).bg(bg)),
                    Span::styled(format!(" {} ", desc), Style::default().fg(Color::Rgb(100, 100, 110)).bg(bg)),
                ])
            }
        })
        .collect();

    frame.render_widget(Paragraph::new(items).style(Style::default().bg(bg)), popup_area);
}

fn render_file_popup(frame: &mut Frame, area: Rect, app: &App) {
    let max_show = 10usize;
    let viewport_top = frame.area().top();
    let max_avail = area.y.saturating_sub(viewport_top);
    
    let count = app.file_completion.candidates.len().min(max_show);
    let popup_height = (count as u16).min(max_avail);
    if popup_height == 0 { return; }

    let popup_y = area.y.saturating_sub(popup_height);
    let popup_width = 50u16.min(area.width.saturating_sub(area.x + 2));
    let popup_area = Rect::new(area.x + 2, popup_y, popup_width, popup_height);

    frame.render_widget(Clear, popup_area);

    let selected = app.file_completion.selected.unwrap_or(0);
    let bg = Color::Rgb(35, 35, 40);
    let sel_bg = Color::Rgb(60, 130, 180);

    let items: Vec<Line> = app.file_completion.candidates
        .iter()
        .take(max_show)
        .enumerate()
        .map(|(i, path)| {
            if i == selected {
                Line::from(Span::styled(format!(" @{} ", path), Style::default().fg(Color::White).bg(sel_bg).add_modifier(Modifier::BOLD)))
            } else {
                Line::from(Span::styled(format!(" @{} ", path), Style::default().fg(Color::Rgb(180, 180, 190)).bg(bg)))
            }
        })
        .collect();

    frame.render_widget(Paragraph::new(items).style(Style::default().bg(bg)), popup_area);
}

fn render_history_search(frame: &mut Frame, area: Rect, app: &App) {
    let match_info = if app.history.search_results.is_empty() {
        "0 results".to_string()
    } else {
        let idx = app.history.search_index.unwrap_or(0) + 1;
        format!("{}/{}", idx, app.history.search_results.len())
    };

    let line = if let Some(idx) = app.history.search_index {
        if let Some(result) = app.history.search_results.get(idx) {
            Line::from(vec![
                Span::styled("bck: ", Style::default().fg(Color::Rgb(100, 180, 255))),
                Span::styled(format!("({}) ", match_info), Style::default().fg(HINT_COLOR)),
                Span::styled(result.as_str(), Style::default().fg(TEXT_COLOR)),
            ])
        } else {
            Line::from(Span::styled("bck: (no match)", Style::default().fg(HINT_COLOR)))
        }
    } else {
        Line::from(vec![
            Span::styled("bck: ", Style::default().fg(Color::Rgb(100, 180, 255))),
            Span::styled(&app.history.search_query, Style::default().fg(TEXT_COLOR)),
            Span::styled("█", Style::default().fg(Color::Rgb(100, 180, 255))),
        ])
    };

    frame.render_widget(Paragraph::new(line), area);
}
