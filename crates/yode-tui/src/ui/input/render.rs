use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::i18n::{text_for, Locale};
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
            &app.session.model,
        ));
        frame.render_widget(paragraph, area);
        if !app.is_thinking && app.pending_confirmation.is_none() {
            frame.set_cursor_position((empty_input_cursor_x(area, &app.session.model), area.y));
        }
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
    model: &str,
) -> Line<'static> {
    let model_badge = Span::styled(
        format!("[{}] ", model),
        Style::default().fg(crate::ui::palette::SELECT_ACCENT),
    );
    let prompt = Span::styled(
        "❯ ",
        Style::default()
            .fg(prompt_color)
            .add_modifier(Modifier::BOLD),
    );
    if let Some(ghost) = ghost_text {
        Line::from(vec![
            model_badge,
            prompt,
            Span::styled(ghost.to_string(), Style::default().fg(GHOST_COLOR)),
        ])
    } else {
        Line::from(vec![
            model_badge,
            prompt,
            Span::styled(fallback_hint, Style::default().fg(HINT_COLOR)),
        ])
    }
}

fn empty_input_cursor_x(area: Rect, model: &str) -> u16 {
    let prefix_width = (model.len() + 5) as u16;
    area.x + prefix_width.min(area.width.saturating_sub(1))
}

fn input_hint_text(app: &App) -> String {
    input_hint_text_for(app, crate::i18n::current_locale())
}

fn input_hint_text_for(app: &App, locale: Locale) -> String {
    if app.is_thinking {
        if app.pending_inputs.is_empty() {
            text_for(locale, "ui.input_queue_followup")
                .unwrap_or("Queue a follow-up…")
                .to_string()
        } else {
            format!(
                "{} {} {}",
                text_for(locale, "ui.input_queue_followup").unwrap_or("Queue a follow-up…"),
                app.pending_inputs.len(),
                text_for(locale, "ui.input_already_queued").unwrap_or("already queued")
            )
        }
    } else {
        text_for(locale, "ui.input_ask_anything")
            .unwrap_or("Ask anything…")
            .to_string()
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

    use super::{empty_input_cursor_x, input_hint_text_for};
    use crate::app::App;
    use crate::i18n::Locale;

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
        assert_eq!(input_hint_text_for(&app, Locale::En), "Ask anything…");

        app.is_thinking = true;
        assert_eq!(input_hint_text_for(&app, Locale::En), "Queue a follow-up…");

        app.pending_inputs
            .push(("next".to_string(), "next".to_string()));
        assert_eq!(
            input_hint_text_for(&app, Locale::En),
            "Queue a follow-up… 1 already queued"
        );
        assert_eq!(
            input_hint_text_for(&app, Locale::ZhCn),
            "继续追加问题… 1 条已排队"
        );
    }

    #[test]
    fn empty_input_cursor_stays_inside_prompt_area() {
        assert_eq!(
            empty_input_cursor_x(ratatui::layout::Rect::new(4, 0, 80, 1), "model"),
            14
        );
        assert_eq!(
            empty_input_cursor_x(ratatui::layout::Rect::new(4, 0, 1, 1), "model"),
            4
        );
        assert_eq!(
            empty_input_cursor_x(ratatui::layout::Rect::new(4, 0, 0, 1), "model"),
            4
        );
    }
}
