use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};
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
        // Render text input, replacing placeholder chars with pill tags
        let max_visible_lines = area.height as usize;
        let mut scroll_y = 0;
        if app.input.cursor_line >= max_visible_lines && max_visible_lines > 0 {
            scroll_y = app.input.cursor_line + 1 - max_visible_lines;
        }

        let mut att_idx = 0usize;
        // Count placeholders in lines before scroll_y
        for line in app.input.lines.iter().take(scroll_y) {
            att_idx += line.chars().filter(|&c| c == '\u{FFFC}').count();
        }

        let lines: Vec<Line> = app.input.lines
            .iter()
            .enumerate()
            .skip(scroll_y)
            .take(max_visible_lines)
            .map(|(i, line)| {
                let prefix = if i == 0 { prompt.clone() } else { Span::raw("  ") };
                let mut spans = vec![prefix];
                // Split line on placeholder chars, inserting pill spans
                let mut buf = String::new();
                for ch in line.chars() {
                    if ch == '\u{FFFC}' {
                        if !buf.is_empty() {
                            spans.push(Span::styled(buf.clone(), Style::default().fg(TEXT_COLOR)));
                            buf.clear();
                        }
                        let pill_text = if let Some(att) = app.input.attachments.get(att_idx) {
                            format!("[{} +{} lines]", att.name, att.line_count)
                        } else {
                            "[paste]".to_string()
                        };
                        spans.push(Span::styled(
                            pill_text,
                            Style::default().fg(Color::Rgb(150, 200, 255)).add_modifier(Modifier::BOLD),
                        ));
                        att_idx += 1;
                    } else {
                        buf.push(ch);
                    }
                }
                if !buf.is_empty() {
                    spans.push(Span::styled(buf, Style::default().fg(TEXT_COLOR)));
                }
                Line::from(spans)
            })
            .collect();

        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    }

    // Cursor
    if !app.is_thinking && app.pending_confirmation.is_none() {
        let term_w = area.width as usize;
        let prefix_w = 2usize; // "❯ " or "  "

        // Calculate visual rows consumed by lines before cursor_line
        let mut visual_y = 0usize;
        let mut pill_idx = 0usize;
        for line in app.input.lines.iter().take(app.input.cursor_line) {
            let line_w: usize = prefix_w + line.chars().map(|ch| {
                if ch == '\u{FFFC}' {
                    let w = app.input.attachments.get(pill_idx)
                        .map(|a| format!("[{} +{} lines]", a.name, a.line_count).len())
                        .unwrap_or(6);
                    pill_idx += 1;
                    w
                } else {
                    UnicodeWidthChar::width(ch).unwrap_or(0)
                }
            }).sum::<usize>();
            visual_y += if line_w == 0 || term_w == 0 { 1 } else { line_w.div_ceil(term_w).max(1) };
        }

        // Display width of cursor line up to cursor_col
        let cursor_content_w: usize = app.input.lines[app.input.cursor_line]
            .chars()
            .take(app.input.cursor_col)
            .map(|c| {
                if c == '\u{FFFC}' {
                    let w = app.input.attachments.get(pill_idx)
                        .map(|a| format!("[{} +{} lines]", a.name, a.line_count).len())
                        .unwrap_or(6);
                    pill_idx += 1;
                    w
                } else {
                    UnicodeWidthChar::width(c).unwrap_or(0)
                }
            })
            .sum();
        let total_x = prefix_w + cursor_content_w;

        // Wrap: which visual row and column
        let wrap_row = if term_w > 0 { total_x / term_w } else { 0 };
        let wrap_col = if term_w > 0 { total_x % term_w } else { total_x };

        frame.set_cursor_position((
            area.x + wrap_col as u16,
            area.y + (visual_y + wrap_row) as u16,
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
