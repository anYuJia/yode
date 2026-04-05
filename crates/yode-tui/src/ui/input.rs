use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthChar;

use crate::app::App;

const PROMPT_COLOR: Color = Color::LightGreen;
const PROMPT_DIM: Color = Color::DarkGray;    // ANSI 8
const TEXT_COLOR: Color = Color::White;        // ANSI 15
const HINT_COLOR: Color = Color::DarkGray;     // ANSI 8 - same as "Done" line
const GHOST_COLOR: Color = Color::DarkGray;    // ANSI 8 - for ghost text

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
        // Show ghost text suggestion if available, otherwise show placeholder
        if let Some(ref ghost) = app.input.ghost_text {
            let paragraph = Paragraph::new(Line::from(vec![
                prompt,
                Span::styled(ghost.clone(), Style::default().fg(GHOST_COLOR)),
            ]));
            frame.render_widget(paragraph, area);
        } else {
            let paragraph = Paragraph::new(Line::from(vec![
                prompt,
                Span::styled("Ask anything…", Style::default().fg(HINT_COLOR)),
            ]));
            frame.render_widget(paragraph, area);
        }
    } else if app.is_thinking && is_empty {
        // Thinking state: show normal prompt (Working indicator is rendered separately above)
        let paragraph = Paragraph::new(Line::from(vec![
            prompt,
            Span::styled("Ask anything…", Style::default().fg(HINT_COLOR)),
        ]));
        frame.render_widget(paragraph, area);
    } else {
        // Render text input with manual character-level wrapping.
        // We can't use ratatui's Wrap (word-level), so we split into visual lines ourselves.
        let term_w = area.width as usize;
        let max_visible = area.height as usize;

        let mut visual_lines: Vec<Line> = Vec::new();
        let mut att_idx = 0usize;

        // Track cursor position during wrapping (avoids separate simulation pass)
        let mut cursor_visual_y = 0usize;
        let mut cursor_col_x = 0usize;

        for (i, logical_line) in app.input.lines.iter().enumerate() {
            let prefix_str = if i == 0 { "❯ " } else { "  " };
            let prefix_w = 2usize;

            // Build (text, style, width) for each item in this logical line
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
                    let pill_text = app.input.pill_display_text(att_idx);
                    let w = pill_text.len();
                    items.push((pill_text, Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD), w));
                    att_idx += 1;
                } else {
                    buf.push(ch);
                }
            }
            if !buf.is_empty() {
                let w = unicode_width::UnicodeWidthStr::width(buf.as_str());
                items.push((buf, Style::default().fg(TEXT_COLOR), w));
            }

            // Track cursor: if this is the cursor line, compute position during wrapping
            let is_cursor_line = i == app.input.cursor_line;
            // For cursor tracking: map char index → (visual_row_within_line, col)
            // We need to simulate wrapping while tracking character positions
            let visual_y_before = visual_lines.len();

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
                        while let Some(&(_byte_i, ch)) = chars.peek() {
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

            // Add ghost text if cursor is at end of last line
            let is_last_line = i == app.input.lines.len() - 1;
            let cursor_at_end = app.input.cursor_col == app.input.char_count();
            if is_last_line && cursor_at_end && app.input.cursor_line == app.input.lines.len() - 1 {
                if let Some(ref ghost) = app.input.ghost_text {
                    row_spans.push(Span::styled(ghost.clone(), Style::default().fg(GHOST_COLOR)));
                }
            }

            if !row_spans.is_empty() {
                visual_lines.push(Line::from(row_spans));
            }

            // Compute cursor position for this line if needed
            if is_cursor_line {
                // Simulate wrapping for chars up to cursor_col to find (row, col)
                let mut pill_scan = 0usize;
                // Count pills in lines before cursor line
                for prev_line in app.input.lines.iter().take(i) {
                    pill_scan += prev_line.chars().filter(|&c| c == '\u{FFFC}').count();
                }
                let mut c_col = prefix_w;
                let mut c_row = 0usize;
                for ch in logical_line.chars().take(app.input.cursor_col) {
                    let cw = if ch == '\u{FFFC}' {
                        let w = app.input.pill_width(pill_scan);
                        pill_scan += 1;
                        w
                    } else {
                        UnicodeWidthChar::width(ch).unwrap_or(0)
                    };
                    if term_w > 0 && c_col + cw > term_w {
                        c_row += 1;
                        c_col = cw;
                    } else {
                        c_col += cw;
                    }
                }
                cursor_visual_y = visual_y_before + c_row;
                cursor_col_x = c_col;
            }
        }

        // Take only the visible portion
        let total = visual_lines.len();
        let skip = total.saturating_sub(max_visible);
        let visible: Vec<Line> = visual_lines.into_iter().skip(skip).take(max_visible).collect();
        frame.render_widget(Paragraph::new(visible), area);

        // Adjust cursor_visual_y for scroll offset
        cursor_visual_y = cursor_visual_y.saturating_sub(skip);

        // Set cursor position (derived from rendering loop, not re-simulated)
        if !app.is_thinking && app.pending_confirmation.is_none() {
            let cursor_y = area.y + cursor_visual_y as u16;
            let max_y = area.y + area.height.saturating_sub(1);
            frame.set_cursor_position((
                area.x + cursor_col_x as u16,
                cursor_y.min(max_y),
            ));
        }
    }

    // File completion popup (still rendered above input)
    if app.file_completion.is_active() {
        render_file_popup(frame, area, app);
    }
}

/// Render attachment pill tags in a separate area below input.
pub fn render_attachments(frame: &mut Frame, area: Rect, app: &App) {
    let mut spans = Vec::new();
    for att in &app.input.attachments {
        spans.push(Span::styled(
            format!("[{} +{} lines] ", att.name, att.line_count),
            Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD),
        ));
    }
    if !spans.is_empty() {
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}

/// Render command completions as an inline list below the input area.
/// Grows from bottom to top, with the best match at the bottom.
pub fn render_command_inline(frame: &mut Frame, area: Rect, app: &App) {
    if area.height == 0 || area.width == 0 { return; }

    let bg = Color::Indexed(235);
    let sel_fg = Color::LightMagenta; // Claude-like selection color
    let sep = "│";

    let show_candidates = &app.cmd_completion.candidates;

    // Args hint mode: single line
    if let Some(hint) = app.cmd_completion.args_hint.as_deref() {
        let items = vec![Line::from(Span::styled(
            format!("  {} ", hint),
            Style::default().fg(Color::Gray).bg(bg),
        ))];
        frame.render_widget(Paragraph::new(items).style(Style::default().bg(bg)), area);
        return;
    }

    let max_show = (area.height as usize).min(5);
    if show_candidates.is_empty() || max_show == 0 { return; }

    let selected = app.cmd_completion.selected.unwrap_or(0);
    let total = show_candidates.len();

    // Use window_start from state, but ensure it's valid
    let window_start = if total <= max_show {
        0
    } else {
        app.cmd_completion.window_start.min(total - max_show)
    };

    // Compute column widths for the visible items
    let max_cmd_len = show_candidates.iter().skip(window_start).take(max_show)
        .map(|(cmd, _)| cmd.len())
        .max()
        .unwrap_or(8);
    let cmd_col_width = max_cmd_len + 1;
    let available_width = area.width as usize;

    // We want the most relevant items (first in list) to be at the BOTTOM.
    // So we take the visible window and reverse them for rendering.
    let mut render_items: Vec<(usize, &(String, String))> = show_candidates
        .iter()
        .enumerate()
        .skip(window_start)
        .take(max_show)
        .collect();
    
    // Reverse so the lowest index (best match) is at the bottom
    render_items.reverse();

    let mut lines: Vec<Line> = render_items
        .into_iter()
        .map(|(i, (cmd, desc))| {
            let desc_max = available_width.saturating_sub(cmd_col_width + 7);
            let desc_truncated: String = if desc.len() > desc_max {
                format!("{}…", &desc[..desc_max.saturating_sub(1)])
            } else {
                desc.to_string()
            };

            if i == selected {
                Line::from(vec![
                    Span::styled(
                        " ❯",
                        Style::default().fg(sel_fg).bg(bg).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{:<width$}", cmd, width = cmd_col_width),
                        Style::default().fg(sel_fg).bg(bg).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(" {} ", sep),
                        Style::default().fg(Color::DarkGray).bg(bg),
                    ),
                    Span::styled(
                        format!("{} ", desc_truncated),
                        Style::default().fg(sel_fg).bg(bg),
                    ),
                ])
            } else {
                Line::from(vec![
                    Span::styled(
                        "  ",
                        Style::default().bg(bg),
                    ),
                    Span::styled(
                        format!("{:<width$}", cmd, width = cmd_col_width),
                        Style::default().fg(Color::Gray).bg(bg),
                    ),
                    Span::styled(
                        format!(" {} ", sep),
                        Style::default().fg(Color::DarkGray).bg(bg),
                    ),
                    Span::styled(
                        format!("{} ", desc_truncated),
                        Style::default().fg(Color::DarkGray).bg(bg),
                    ),
                ])
            }
        })
        .collect();

    // If we have fewer candidates than area.height, we need to push them to the bottom.
    if lines.len() < area.height as usize {
        let diff = area.height as usize - lines.len();
        let mut padded = Vec::with_capacity(area.height as usize);
        for _ in 0..diff {
            padded.push(Line::from(Span::styled(" ".repeat(area.width as usize), Style::default().bg(bg))));
        }
        padded.extend(lines);
        lines = padded;
    }

    frame.render_widget(Paragraph::new(lines).style(Style::default().bg(bg)), area);
}

fn render_file_popup(frame: &mut Frame, area: Rect, app: &App) {
    let viewport_top = frame.area().top();
    let max_avail = area.y.saturating_sub(viewport_top) as usize;
    let max_show = 10usize.min(max_avail);
    if max_show == 0 { return; }

    let total = app.file_completion.candidates.len();
    let popup_height = total.min(max_show) as u16;
    let popup_y = area.y.saturating_sub(popup_height);
    let popup_width = 50u16.min(area.width.saturating_sub(area.x + 2));
    let popup_area = Rect::new(area.x + 2, popup_y, popup_width, popup_height);

    frame.render_widget(Clear, popup_area);

    let selected = app.file_completion.selected.unwrap_or(0);
    let bg = Color::Indexed(235);
    let sel_fg = Color::LightMagenta;

    // Use window_start from state
    let window_start = if total <= max_show {
        0
    } else {
        app.file_completion.window_start.min(total - max_show)
    };

    let items: Vec<Line> = app.file_completion.candidates
        .iter()
        .enumerate()
        .skip(window_start)
        .take(max_show)
        .map(|(i, path)| {
            if i == selected {
                Line::from(vec![
                    Span::styled(" ❯ ", Style::default().fg(sel_fg).bg(bg).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("@{} ", path), Style::default().fg(sel_fg).bg(bg).add_modifier(Modifier::BOLD)),
                ])
            } else {
                Line::from(vec![
                    Span::styled("   ", Style::default().bg(bg)),
                    Span::styled(format!("@{} ", path), Style::default().fg(Color::Gray).bg(bg)),
                ])
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
                Span::styled("bck: ", Style::default().fg(Color::LightBlue)),
                Span::styled(format!("({}) ", match_info), Style::default().fg(HINT_COLOR)),
                Span::styled(result.as_str(), Style::default().fg(TEXT_COLOR)),
            ])
        } else {
            Line::from(Span::styled("bck: (no match)", Style::default().fg(HINT_COLOR)))
        }
    } else {
        Line::from(vec![
            Span::styled("bck: ", Style::default().fg(Color::LightBlue)),
            Span::styled(&app.history.search_query, Style::default().fg(TEXT_COLOR)),
            Span::styled("█", Style::default().fg(Color::LightBlue)),
        ])
    };

    frame.render_widget(Paragraph::new(line), area);
}
