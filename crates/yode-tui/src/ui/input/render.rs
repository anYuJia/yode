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

    if is_empty {
        let paragraph = Paragraph::new(ghost_or_hint_line(
            prompt_color,
            app.input.ghost_text.as_deref(),
            input_hint_text(app),
        ));
        frame.render_widget(paragraph, area);
    } else {
        render_wrapped_input(frame, area, app, prompt_color);
    }

    if app.file_completion.is_active() {
        render_file_popup(frame, area, app);
    }
}

fn ghost_or_hint_line(
    prompt_color: Color,
    ghost_text: Option<&str>,
    fallback_hint: String,
) -> Line<'static> {
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
            Span::styled(fallback_hint, Style::default().fg(HINT_COLOR)),
        ])
    }
}

fn input_hint_text(app: &App) -> String {
    if app.is_thinking {
        if app.pending_inputs.is_empty() {
            "Queue a follow-up…".to_string()
        } else {
            format!(
                "Queue a follow-up… {} already queued",
                app.pending_inputs.len()
            )
        }
    } else {
        "Ask anything…".to_string()
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use yode_llm::registry::ProviderRegistry;
    use yode_tools::registry::ToolRegistry;

    use super::input_hint_text;
    use crate::app::App;

    fn test_app() -> App {
        App::new(
            "test-model".to_string(),
            "session-1234".to_string(),
            "/tmp".to_string(),
            "test".to_string(),
            Vec::new(),
            HashMap::new(),
            Arc::new(ProviderRegistry::new()),
            Arc::new(ToolRegistry::new()),
        )
    }

    #[test]
    fn input_hint_reflects_queueing_while_model_is_working() {
        let mut app = test_app();
        assert_eq!(input_hint_text(&app), "Ask anything…");

        app.is_thinking = true;
        assert_eq!(input_hint_text(&app), "Queue a follow-up…");

        app.pending_inputs
            .push(("next".to_string(), "next".to_string()));
        assert_eq!(input_hint_text(&app), "Queue a follow-up… 1 already queued");
    }
}
