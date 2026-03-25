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

    let is_empty = app.input.is_empty();

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
        let lines: Vec<Line> = app.input.lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                if i == 0 {
                    Line::from(vec![
                        prompt.clone(),
                        Span::styled(line.as_str(), Style::default().fg(TEXT_COLOR)),
                    ])
                } else {
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled(line.as_str(), Style::default().fg(TEXT_COLOR)),
                    ])
                }
            })
            .collect();

        frame.render_widget(Paragraph::new(lines), area);
    }

    // Cursor
    if !app.is_thinking && app.pending_confirmation.is_none() {
        let display_width: usize = app.input.lines[app.input.cursor_line]
            .chars()
            .take(app.input.cursor_col)
            .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
            .sum();
        let cursor_x = area.x + 2 + display_width as u16;
        let cursor_y = area.y + app.input.cursor_line as u16;
        if cursor_y < area.y + area.height {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }

    // Completion popups
    if app.file_completion.is_active() {
        render_file_popup(frame, area, app);
    } else if app.cmd_completion.is_active() {
        render_command_popup(frame, area, app);
    }
}

fn render_command_popup(frame: &mut Frame, area: Rect, app: &App) {
    let max_show = 8usize;
    let count = app.cmd_completion.candidates.len().min(max_show);
    let popup_height = count as u16;
    let popup_y = area.y.saturating_sub(popup_height);
    let popup_width = 48u16.min(area.width);
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
    let count = app.file_completion.candidates.len().min(max_show);
    let popup_height = count as u16;
    let popup_y = area.y.saturating_sub(popup_height);
    let popup_width = 50u16.min(area.width);
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
