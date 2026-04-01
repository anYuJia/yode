pub mod chat;
pub mod input;
pub mod status_bar;
pub mod tool_confirm;

use ratatui::Frame;

use crate::app::App;

/// Viewport is dynamically resized to exactly fit content.
/// Grows upward when input lines increase, shrinks downward when they decrease.
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
        let input_lines = app.input.line_count() as u16;
        let attachment_lines = app.input.attachments.len() as u16;
        let input_height = (input_lines + attachment_lines).clamp(1, 5);

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
