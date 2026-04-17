mod badges;
pub mod chat;
pub(crate) mod chat_entries;
mod chat_header;
pub(crate) mod chat_layout;
mod chat_markdown;
mod highlighted_code;
pub mod input;
pub(crate) mod inspector;
mod layout;
mod palette;
pub(crate) mod panels;
mod pending;
mod responsive;
pub(crate) mod status_summary;
pub mod status_bar;
mod structured_diff;
pub mod tool_confirm;
mod turn_status;
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
        let panel_area = panels::centered_panel_rect(frame.area(), 84, 4);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(panel_area);

        tool_confirm::render_inline_confirm(frame, &chunks, app);
    } else if let Some(inspector) = app.inspector.views.last() {
        inspector::render_inspector(frame, frame.area(), &inspector.document);
    } else {
        let plan = layout::build_main_layout(frame.area(), app);
        if plan.show_completion {
            input::render_command_inline(frame, plan.areas[0], app);
            pending::render_pending_inputs(frame, plan.areas[1], app);
            status_bar::render_separator(frame, plan.areas[2]);
            input::render_input(frame, plan.areas[3], app);
            status_bar::render_separator(frame, plan.areas[4]);
            status_bar::render_info_line(frame, plan.areas[5], app);
            status_bar::render_blank_line(frame, plan.areas[6], app);
        } else {
            if plan.show_turn_status {
                turn_status::render_turn_status(frame, plan.areas[0], app);
            }
            pending::render_pending_inputs(frame, plan.areas[1], app);
            status_bar::render_separator(frame, plan.areas[2]);
            input::render_input(frame, plan.areas[3], app);
            status_bar::render_separator(frame, plan.areas[4]);
            status_bar::render_info_line(frame, plan.areas[5], app);
            status_bar::render_blank_line(frame, plan.areas[6], app);
        }
    }
}
