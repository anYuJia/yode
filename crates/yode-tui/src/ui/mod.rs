pub mod chat;
pub mod input;
pub mod status_bar;
pub mod tool_confirm;

use ratatui::style::{Color, Style};
use ratatui::Frame;

use crate::app::App;

/// Viewport layout (4 lines):
///   ────────────────────────────────────────
///   ❯ input prompt
///   ● normal · 87 tok · ctx 3% · /help
///                                        (bottom padding)
pub fn render(frame: &mut Frame, app: &mut App) {
    use ratatui::layout::{Constraint, Direction, Layout};

    if app.pending_confirmation.is_some() {
        // Confirmation mode: use all 4 lines for inline vertical selector
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Tool name header
                Constraint::Length(1), // Option 1: Yes
                Constraint::Length(1), // Option 2: Always allow
                Constraint::Length(1), // Option 3: No
            ])
            .split(frame.area());

        tool_confirm::render_inline_confirm(frame, &chunks, app);
    } else {
        // Normal mode
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Separator
                Constraint::Length(1), // Input
                Constraint::Length(1), // Info line
                Constraint::Length(1), // Bottom padding
            ])
            .split(frame.area());

        status_bar::render_separator(frame, chunks[0]);
        input::render_input(frame, chunks[1], app);
        status_bar::render_info_line(frame, chunks[2], app);

        // Force-render bottom padding: set a non-default fg color on every cell
        // so ratatui's diff renderer actually writes them to the terminal.
        frame.buffer_mut().set_style(
            chunks[3],
            Style::default().fg(Color::DarkGray),
        );
    }
}
