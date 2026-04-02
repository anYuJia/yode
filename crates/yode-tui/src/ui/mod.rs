pub mod chat;
pub mod input;
pub mod status_bar;
pub mod tool_confirm;

use ratatui::Frame;

use crate::app::App;

/// Viewport is dynamically resized to exactly fit content.
/// Long lines wrap automatically; input height adapts to visual line count.
pub fn render(frame: &mut Frame, app: &mut App) {
    use ratatui::layout::{Constraint, Direction, Layout};

    if app.pending_confirmation.is_some() {
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
                    Constraint::Length(input_height),
                    Constraint::Length(1),
                    Constraint::Length(1),
                ])
                .split(frame.area());

            input::render_command_inline(frame, chunks[0], app);
            input::render_input(frame, chunks[1], app);
            status_bar::render_info_line(frame, chunks[2], app);
        } else {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(input_height),
                    Constraint::Length(1),
                    Constraint::Length(1),
                ])
                .split(frame.area());

            input::render_input(frame, chunks[0], app);
            status_bar::render_info_line(frame, chunks[1], app);
        }
    }
}
