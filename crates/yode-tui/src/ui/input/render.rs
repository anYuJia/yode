use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::ui::palette::{GHOST_COLOR, HINT_COLOR, PROMPT_COLOR, PROMPT_DIM, TEXT_COLOR};

use super::completions::{render_file_popup, render_history_search};
use super::wrapping::build_wrapped_input_layout;

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
    let is_empty = app.input.is_empty() && app.input.attachments.is_empty();

    if is_empty && !app.is_thinking {
        let paragraph = Paragraph::new(ghost_or_hint_line(prompt_color, app.input.ghost_text.as_deref()));
        frame.render_widget(paragraph, area);
    } else if app.is_thinking && is_empty {
        let paragraph = Paragraph::new(ghost_or_hint_line(prompt_color, None));
        frame.render_widget(paragraph, area);
    } else {
        render_wrapped_input(frame, area, app, prompt_color);
    }

    if app.file_completion.is_active() {
        render_file_popup(frame, area, app);
    }
}

fn ghost_or_hint_line(prompt_color: Color, ghost_text: Option<&str>) -> Line<'static> {
    let prompt = Span::styled(
        "❯ ",
        Style::default()
            .fg(prompt_color)
            .add_modifier(Modifier::BOLD),
    );
    if let Some(ghost) = ghost_text {
        Line::from(vec![
            prompt,
            Span::styled(ghost.to_string(), Style::default().fg(GHOST_COLOR)),
        ])
    } else {
        Line::from(vec![
            prompt,
            Span::styled("Ask anything…", Style::default().fg(HINT_COLOR)),
        ])
    }
}

fn render_wrapped_input(frame: &mut Frame, area: Rect, app: &App, prompt_color: Color) {
    let term_width = area.width as usize;
    let max_visible = area.height as usize;
    let layout = build_wrapped_input_layout(
        app,
        term_width,
        Style::default()
            .fg(prompt_color)
            .add_modifier(Modifier::BOLD),
        Style::default().fg(TEXT_COLOR),
        Style::default()
            .fg(Color::LightCyan)
            .add_modifier(Modifier::BOLD),
        Style::default().fg(GHOST_COLOR),
    );

    let total = layout.lines.len();
    let skip = total.saturating_sub(max_visible);
    let visible: Vec<Line> = layout
        .lines
        .into_iter()
        .skip(skip)
        .take(max_visible)
        .collect();
    frame.render_widget(Paragraph::new(visible), area);

    let cursor_visual_y = layout.cursor_visual_y.saturating_sub(skip);
    if !app.is_thinking && app.pending_confirmation.is_none() {
        let cursor_y = area.y + cursor_visual_y as u16;
        let max_y = area.y + area.height.saturating_sub(1);
        frame.set_cursor_position((area.x + layout.cursor_col_x as u16, cursor_y.min(max_y)));
    }
}
