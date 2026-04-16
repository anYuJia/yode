use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::ui::palette::{HINT_COLOR, TEXT_COLOR};
use super::formatting::{
    completion_candidate_line, truncate_ellipsis, COMPLETION_BG, COMPLETION_SELECTED_BG,
    COMPLETION_SELECTED_FG,
};


/// Render command completions as an inline list below the input area.
/// Grows from bottom to top, with the best match at the bottom.
pub fn render_command_inline(frame: &mut Frame, area: Rect, app: &App) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let bg = COMPLETION_BG;
    let candidates = &app.cmd_completion.candidates;

    if let Some(hint) = app.cmd_completion.args_hint.as_deref() {
        let items = vec![Line::from(Span::styled(
            format!("  {} ", hint),
            Style::default().fg(Color::Gray).bg(bg),
        ))];
        frame.render_widget(Paragraph::new(items).style(Style::default().bg(bg)), area);
        return;
    }

    let max_show = (area.height as usize).min(5);
    if candidates.is_empty() || max_show == 0 {
        return;
    }

    let selected = app.cmd_completion.selected.unwrap_or(0);
    let total = candidates.len();
    let window_start = if total <= max_show {
        0
    } else {
        app.cmd_completion.window_start.min(total - max_show)
    };
    let max_cmd_len = candidates
        .iter()
        .skip(window_start)
        .take(max_show)
        .map(|(command, _)| command.len())
        .max()
        .unwrap_or(8);
    let command_width = max_cmd_len + 1;
    let available_width = area.width as usize;

    let mut render_items: Vec<(usize, &(String, String))> = candidates
        .iter()
        .enumerate()
        .skip(window_start)
        .take(max_show)
        .collect();
    render_items.reverse();

    let mut lines: Vec<Line> = render_items
        .into_iter()
        .map(|(index, (command, description))| {
            completion_candidate_line(
                index == selected,
                command,
                description,
                command_width,
                available_width,
            )
        })
        .collect();

    if lines.len() < area.height as usize {
        let diff = area.height as usize - lines.len();
        let mut padded = Vec::with_capacity(area.height as usize);
        for _ in 0..diff {
            padded.push(Line::from(Span::styled(
                " ".repeat(area.width as usize),
                Style::default().bg(bg),
            )));
        }
        padded.extend(lines);
        lines = padded;
    }

    frame.render_widget(Paragraph::new(lines).style(Style::default().bg(bg)), area);
}

pub(super) fn render_file_popup(frame: &mut Frame, area: Rect, app: &App) {
    let viewport_top = frame.area().top();
    let max_available = area.y.saturating_sub(viewport_top) as usize;
    let max_show = 10usize.min(max_available);
    if max_show == 0 {
        return;
    }

    let total = app.file_completion.candidates.len();
    let popup_height = total.min(max_show) as u16;
    let popup_y = area.y.saturating_sub(popup_height);
    let popup_width = 50u16.min(area.width.saturating_sub(area.x + 2));
    let popup_area = Rect::new(area.x + 2, popup_y, popup_width, popup_height);

    frame.render_widget(Clear, popup_area);

    let selected = app.file_completion.selected.unwrap_or(0);
    let bg = COMPLETION_BG;
    let selected_fg = COMPLETION_SELECTED_FG;
    let selected_bg = COMPLETION_SELECTED_BG;
    let window_start = if total <= max_show {
        0
    } else {
        app.file_completion.window_start.min(total - max_show)
    };

    let items: Vec<Line> = app
        .file_completion
        .candidates
        .iter()
        .enumerate()
        .skip(window_start)
        .take(max_show)
        .map(|(index, path)| {
            if index == selected {
                Line::from(vec![
                    Span::styled(
                        " ❯ ",
                        Style::default()
                            .fg(selected_fg)
                            .bg(selected_bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("@{} ", truncate_ellipsis(path, 40)),
                        Style::default()
                            .fg(selected_fg)
                            .bg(selected_bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                ])
            } else {
                Line::from(vec![
                    Span::styled("   ", Style::default().bg(bg)),
                    Span::styled(
                        format!("@{} ", truncate_ellipsis(path, 40)),
                        Style::default().fg(Color::Gray).bg(bg),
                    ),
                ])
            }
        })
        .collect();

    frame.render_widget(
        Paragraph::new(items).style(Style::default().bg(bg)),
        popup_area,
    );
}

pub(super) fn render_history_search(frame: &mut Frame, area: Rect, app: &App) {
    let match_info = if app.history.search_results().is_empty() {
        "0 results".to_string()
    } else {
        let index = app.history.search_index().unwrap_or(0) + 1;
        format!("{}/{}", index, app.history.search_results().len())
    };

    let line = if let Some(result) = app.history.current_search_result() {
        Line::from(vec![
            Span::styled("bck: ", Style::default().fg(Color::LightBlue)),
            Span::styled(
                format!("({}) ", match_info),
                Style::default().fg(HINT_COLOR),
            ),
            Span::styled(result, Style::default().fg(TEXT_COLOR)),
        ])
    } else {
        Line::from(vec![
            Span::styled("bck: ", Style::default().fg(Color::LightBlue)),
            Span::styled(app.history.search_query(), Style::default().fg(TEXT_COLOR)),
            Span::styled("█", Style::default().fg(Color::LightBlue)),
        ])
    };

    frame.render_widget(Paragraph::new(line), area);
}
