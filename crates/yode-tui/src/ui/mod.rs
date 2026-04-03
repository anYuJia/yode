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
        let thinking_height: u16 = if app.is_thinking { 1 } else { 0 };

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
                    Constraint::Length(thinking_height),
                    Constraint::Length(1), // separator above input
                    Constraint::Length(input_height),
                    Constraint::Length(1), // separator above status
                    Constraint::Length(1),
                ])
                .split(frame.area());

            input::render_command_inline(frame, chunks[0], app);
            render_thinking_line(frame, chunks[1], app);
            status_bar::render_separator(frame, chunks[2]);
            input::render_input(frame, chunks[3], app);
            status_bar::render_separator(frame, chunks[4]);
            status_bar::render_info_line(frame, chunks[5], app);
        } else {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(thinking_height),
                    Constraint::Length(1), // separator above input
                    Constraint::Length(input_height),
                    Constraint::Length(1), // separator above status
                    Constraint::Length(1),
                ])
                .split(frame.area());

            render_thinking_line(frame, chunks[0], app);
            status_bar::render_separator(frame, chunks[1]);
            input::render_input(frame, chunks[2], app);
            status_bar::render_separator(frame, chunks[3]);
            status_bar::render_info_line(frame, chunks[4], app);
        }
    }
}

/// Render the "Working…" indicator line above the input separator.
fn render_thinking_line(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    if !app.is_thinking || area.height == 0 { return; }
    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;

    let spinner = app.spinner_char();
    let verb = app.thinking.verb;
    let elapsed_str = app.thinking_elapsed_str();
    let queue_info = if !app.pending_inputs.is_empty() {
        format!(" ({} queued)", app.pending_inputs.len())
    } else {
        String::new()
    };

    let line = Line::from(vec![
        Span::styled(
            format!("  {} ", spinner),
            Style::default().fg(Color::Yellow),
        ),
        Span::styled(
            format!("{}… ({}{})", verb, elapsed_str, queue_info),
            Style::default().fg(Color::Gray),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}
