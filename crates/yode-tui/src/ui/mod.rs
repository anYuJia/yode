pub mod chat;
mod chat_markdown;
pub mod input;
pub mod status_bar;
pub mod tool_confirm;
pub mod wizard;

use ratatui::Frame;

use crate::app::App;

/// Viewport is dynamically resized to exactly fit content.
/// Long lines wrap automatically; input height adapts to visual line count.
pub fn render(frame: &mut Frame, app: &mut App) {
    use ratatui::layout::{Constraint, Direction, Layout};

    if app.wizard.is_some() {
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
        let term_width = frame.area().width;
        let visual_lines = app.input.visual_line_count(term_width) as u16;
        let input_height = visual_lines.clamp(1, 5);
        let status_height_raw: u16 = if app.turn_status.is_visible() { 1 } else { 0 };

        let pending_height = app.pending_inputs.len() as u16;

        let completion_height = if app.cmd_completion.is_active() {
            if app.cmd_completion.args_hint.is_some() {
                1
            } else if !app.cmd_completion.candidates.is_empty() {
                5
            } else {
                0
            }
        } else {
            0
        };

        // If completion is active, we hide the turn status to save space and avoid clutter.
        // Otherwise, if status is visible, we give it 3 lines (blank + status + blank).
        let status_area_height = if completion_height > 0 {
            0
        } else if status_height_raw > 0 {
            3
        } else {
            0
        };

        if completion_height > 0 {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(completion_height),
                    Constraint::Length(pending_height), // Queued inputs
                    Constraint::Length(1),              // separator above input
                    Constraint::Length(input_height),
                    Constraint::Length(1), // separator above status bar
                    Constraint::Length(1), // status bar
                    Constraint::Length(1), // blank line
                ])
                .split(frame.area());

            input::render_command_inline(frame, chunks[0], app);
            render_pending_inputs(frame, chunks[1], app);
            status_bar::render_separator(frame, chunks[2]);
            input::render_input(frame, chunks[3], app);
            status_bar::render_separator(frame, chunks[4]);
            status_bar::render_info_line(frame, chunks[5], app);
            status_bar::render_blank_line(frame, chunks[6], app);
        } else {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(status_area_height),
                    Constraint::Length(pending_height), // Queued inputs
                    Constraint::Length(1),              // separator above input
                    Constraint::Length(input_height),
                    Constraint::Length(1), // separator above status bar
                    Constraint::Length(1), // status bar
                    Constraint::Length(1), // blank line
                ])
                .split(frame.area());

            if status_area_height > 0 {
                render_turn_status(frame, chunks[0], app);
            }
            render_pending_inputs(frame, chunks[1], app);
            status_bar::render_separator(frame, chunks[2]);
            input::render_input(frame, chunks[3], app);
            status_bar::render_separator(frame, chunks[4]);
            status_bar::render_info_line(frame, chunks[5], app);
            status_bar::render_blank_line(frame, chunks[6], app);
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
