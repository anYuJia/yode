pub mod chat;
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
            .constraints([
                Constraint::Length(wiz_height),
                Constraint::Length(1),
            ])
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
        let status_height: u16 = if app.turn_status.is_visible() { 1 } else { 0 };

        let completion_height = if app.cmd_completion.is_active() && !app.cmd_completion.candidates.is_empty() {
            (app.cmd_completion.candidates.len() as u16).min(5)
        } else {
            0
        };

        if completion_height > 0 {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(completion_height),
                    Constraint::Length(status_height),
                    Constraint::Length(1), // separator above input
                    Constraint::Length(input_height),
                    Constraint::Length(1), // separator above status bar
                    Constraint::Length(1),
                ])
                .split(frame.area());

            input::render_command_inline(frame, chunks[0], app);
            render_turn_status(frame, chunks[1], app);
            status_bar::render_separator(frame, chunks[2]);
            input::render_input(frame, chunks[3], app);
            status_bar::render_separator(frame, chunks[4]);
            status_bar::render_info_line(frame, chunks[5], app);
        } else {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(status_height),
                    Constraint::Length(1), // separator above input
                    Constraint::Length(input_height),
                    Constraint::Length(1), // separator above status bar
                    Constraint::Length(1),
                ])
                .split(frame.area());

            render_turn_status(frame, chunks[0], app);
            status_bar::render_separator(frame, chunks[1]);
            input::render_input(frame, chunks[2], app);
            status_bar::render_separator(frame, chunks[3]);
            status_bar::render_info_line(frame, chunks[4], app);
        }
    }
}

/// Render the unified turn status line with blank lines above/below.
fn render_turn_status(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    if area.height == 0 { return; }
    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;
    use crate::app::TurnStatus;

    let status_line = match &app.turn_status {
        TurnStatus::Idle => return,

        TurnStatus::Working { verb } => {
            let spinner = app.spinner_char();
            let elapsed = app.thinking_elapsed_str();
            let input_tok = app.session.input_tokens;
            // Estimate output tokens from streaming buffer (real-time)
            let stream_chars = app.streaming_buf.len() as u32;
            let output_tok = app.session.output_tokens + stream_chars / 4;

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
                    format!(" ({} · ↑{} ↓{} tok)", elapsed, format_tok(input_tok), format_tok(output_tok)),
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        }

        TurnStatus::Done { elapsed, tools } => {
            let elapsed_str = crate::app::format_duration(*elapsed);
            let tools_str = if *tools > 0 {
                format!(" · {} tool calls", tools)
            } else {
                String::new()
            };
            let input_tok = app.session.input_tokens;
            let output_tok = app.session.output_tokens;
            Line::from(vec![
                Span::styled(
                    format!("  ⚡ Done · {}{} (↑{} ↓{} tok)", elapsed_str, tools_str, format_tok(input_tok), format_tok(output_tok)),
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        }

        TurnStatus::Retrying { error, attempt, max_attempts, delay_secs } => {
            Line::from(vec![
                Span::styled(
                    format!("  ⎿ {}", error),
                    Style::default().fg(Color::LightRed),
                ),
                Span::styled(
                    format!(" · Retrying in {}s ({}/{})", delay_secs, attempt, max_attempts),
                    Style::default().fg(Color::Yellow),
                ),
            ])
        }
    };

    // Render status line directly (no blank line padding — viewport is tight)
    let lines = vec![status_line];
    frame.render_widget(Paragraph::new(lines), area);
}

/// Format token count: 1234 → "1.2k", 500 → "500"
fn format_tok(n: u32) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}
