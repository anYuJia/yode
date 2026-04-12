pub mod chat;
mod chat_entries;
pub(crate) mod chat_layout;
mod chat_markdown;
mod layout;
mod palette;
mod responsive;
pub mod input;
pub mod status_bar;
pub mod tool_confirm;
pub mod wizard;

use ratatui::Frame;

use crate::app::App;

/// Viewport is dynamically resized to exactly fit content.
/// Long lines wrap automatically; input height adapts to visual line count.
pub fn render(frame: &mut Frame, app: &mut App) {
    if app.wizard.is_some() {
        use ratatui::layout::{Constraint, Direction, Layout};
        // Wizard mode: dedicated UI
        let wiz = app.wizard.as_ref().unwrap();
        let wiz_height = wiz.viewport_height();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(wiz_height), Constraint::Length(1)])
            .split(frame.area());

        wizard::render_wizard(frame, chunks[0], wiz);
        status_bar::render_info_line(frame, chunks[1], app);
    } else if app.pending_confirmation.is_some() {
        use ratatui::layout::{Constraint, Direction, Layout};
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(frame.area());

        tool_confirm::render_inline_confirm(frame, &chunks, app);
    } else {
        let plan = layout::build_main_layout(frame.area(), app);
        if plan.show_completion {
            input::render_command_inline(frame, plan.areas[0], app);
            render_pending_inputs(frame, plan.areas[1], app);
            status_bar::render_separator(frame, plan.areas[2]);
            input::render_input(frame, plan.areas[3], app);
            status_bar::render_separator(frame, plan.areas[4]);
            status_bar::render_info_line(frame, plan.areas[5], app);
            status_bar::render_blank_line(frame, plan.areas[6], app);
        } else {
            if plan.show_turn_status {
                render_turn_status(frame, plan.areas[0], app);
            }
            render_pending_inputs(frame, plan.areas[1], app);
            status_bar::render_separator(frame, plan.areas[2]);
            input::render_input(frame, plan.areas[3], app);
            status_bar::render_separator(frame, plan.areas[4]);
            status_bar::render_info_line(frame, plan.areas[5], app);
            status_bar::render_blank_line(frame, plan.areas[6], app);
        }
    }
}

fn render_pending_inputs(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    if area.height == 0 || app.pending_inputs.is_empty() {
        return;
    }
    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;

    let mut lines = Vec::new();
    for (display, _) in &app.pending_inputs {
        // Truncate long queued inputs to 1 line for preview
        let text = display.lines().next().unwrap_or("").to_string();
        let display_text = if text.len() > 80 {
            format!("{}...", &text[..80])
        } else {
            text
        };
        lines.push(Line::from(vec![
            Span::styled("  > ", Style::default().fg(Color::DarkGray)),
            Span::styled(display_text, Style::default().fg(Color::DarkGray)),
            Span::styled(" (queued)", Style::default().fg(Color::DarkGray)),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// Render the unified turn status line with blank lines above/below.
fn render_turn_status(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    if area.height == 0 {
        return;
    }
    use crate::app::TurnStatus;
    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;
    let indicators = turn_runtime_indicator_text(app);

    let status_line = match &app.turn_status {
        TurnStatus::Idle => return,

        TurnStatus::Working { verb } => {
            let spinner = app.spinner_char();
            let elapsed = app.thinking_elapsed_str();
            // Add streaming buffer estimate to turn output tokens
            let stream_chars = app.streaming_buf.len() as u32;
            let output_tok = app.session.turn_output_tokens + stream_chars / 4;

            Line::from(vec![
                Span::styled(
                    format!("  {} ", spinner),
                    Style::default().fg(Color::LightMagenta),
                ),
                Span::styled(
                    format!("{}…", verb),
                    Style::default().fg(Color::LightMagenta),
                ),
                Span::styled(
                    format!(" ({} · ↓{} tokens)", elapsed, format_tok(output_tok)),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(indicators.clone(), Style::default().fg(Color::DarkGray)),
            ])
        }

        TurnStatus::Done { elapsed, tools } => {
            let elapsed_str = crate::app::format_duration(*elapsed);
            let tools_str = if *tools > 0 {
                format!(" · {} tool calls", tools)
            } else {
                String::new()
            };
            // Show per-turn output tokens only (Claude-style)
            let turn_out = app.session.turn_output_tokens;
            Line::from(vec![Span::styled(
                format!(
                    "  ⚡ Done · {}{} (↓{} tokens){}",
                    elapsed_str,
                    tools_str,
                    format_tok(turn_out),
                    indicators
                ),
                Style::default().fg(Color::DarkGray),
            )])
        }

        TurnStatus::Retrying {
            error,
            attempt,
            max_attempts,
            delay_secs,
        } => Line::from(vec![
            Span::styled(
                format!("  ⎿ {}", error),
                Style::default().fg(Color::LightRed),
            ),
            Span::styled(
                format!(
                    " · Retrying in {}s ({}/{})",
                    delay_secs, attempt, max_attempts
                ),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(indicators, Style::default().fg(Color::DarkGray)),
        ]),
    };

    // Render status line with blank lines for padding if height allows
    let lines = if area.height >= 3 {
        vec![Line::from(""), status_line, Line::from("")]
    } else {
        vec![status_line]
    };
    frame.render_widget(Paragraph::new(lines), area);
}

fn turn_runtime_indicator_text(app: &App) -> String {
    let Some(engine) = &app.engine else {
        return String::new();
    };
    let Ok(engine) = engine.try_lock() else {
        return String::new();
    };
    let state = engine.runtime_state();
    let mem = if state.live_session_memory_updating {
        format!("mem {}*", state.session_memory_update_count)
    } else if state.live_session_memory_initialized {
        format!("mem {}", state.session_memory_update_count)
    } else {
        "mem cold".to_string()
    };
    let recovery = match state.recovery_state.as_str() {
        "ReanchorRequired" => " · reanchor",
        "SingleStepMode" => " · single-step",
        "NeedUserGuidance" => " · ask-user",
        _ => "",
    };
    format!(
        " · compact {} · {}{}",
        state.total_compactions, mem, recovery
    )
}

/// Format token count: 1234 → "1.2k", 500 → "500"
fn format_tok(n: u32) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}
