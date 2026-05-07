mod badges;
pub mod chat;
pub(crate) mod chat_entries;
mod chat_header;
pub(crate) mod chat_layout;
mod chat_markdown;
pub(crate) mod error_format;
mod highlighted_code;
pub mod input;
pub(crate) mod inspector;
pub(crate) mod layout;
mod palette;
pub(crate) mod panels;
mod pending;
mod responsive;
pub mod status_bar;
pub(crate) mod status_summary;
mod structured_diff;
pub mod tool_confirm;
mod turn_status;
pub mod wizard;

use ratatui::Frame;

use crate::app::App;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderMode {
    Wizard,
    Inspector,
    Confirmation,
    Main,
}

pub(crate) const INSPECTOR_STATUS_HEIGHT: u16 = 1;

/// Viewport is dynamically resized to exactly fit content.
/// Long lines wrap automatically; input height adapts to visual line count.
pub fn render(frame: &mut Frame, app: &mut App) {
    match render_mode(app) {
        RenderMode::Wizard => render_wizard_mode(frame, app),
        RenderMode::Inspector => render_inspector_mode(frame, app),
        RenderMode::Confirmation => render_confirmation_mode(frame, app),
        RenderMode::Main => render_main_mode(frame, app),
    }
}

fn render_mode(app: &App) -> RenderMode {
    if app.wizard.is_some() {
        RenderMode::Wizard
    } else if app.inspector.views.last().is_some() {
        RenderMode::Inspector
    } else if app.pending_confirmation.is_some() {
        RenderMode::Confirmation
    } else {
        RenderMode::Main
    }
}

fn render_wizard_mode(frame: &mut Frame, app: &mut App) {
    if let Some(wiz) = app.wizard.as_ref() {
        use ratatui::layout::{Constraint, Direction, Layout};
        // Wizard mode: dedicated UI
        let wiz_height = wiz.viewport_height();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(wiz_height), Constraint::Length(1)])
            .split(frame.area());

        wizard::render_wizard(frame, chunks[0], wiz);
        status_bar::render_info_line(frame, chunks[1], app);
    }
}

fn render_inspector_mode(frame: &mut Frame, app: &mut App) {
    if let Some(inspector) = app.inspector.views.last() {
        use ratatui::layout::{Constraint, Direction, Layout};

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(INSPECTOR_STATUS_HEIGHT),
            ])
            .split(frame.area());
        inspector::render_inspector(frame, chunks[0], &inspector.document);
        status_bar::render_info_line(frame, chunks[1], app);
    }
}

fn render_confirmation_mode(frame: &mut Frame, app: &mut App) {
    use ratatui::layout::{Constraint, Direction, Layout};

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(tool_confirm::INLINE_CONFIRM_HEIGHT),
        ])
        .split(frame.area());
    status_bar::render_info_line(frame, chunks[1], app);
    tool_confirm::render_inline_confirm(frame, chunks[2], app);
}

fn render_main_mode(frame: &mut Frame, app: &mut App) {
    let plan = layout::build_main_layout(frame.area(), app);
    if plan.show_completion {
        input::render_command_inline(frame, plan.areas[0], app);
        if plan.show_turn_status {
            turn_status::render_turn_status(frame, plan.areas[1], app);
        }
        pending::render_pending_inputs(frame, plan.areas[2], app);
        status_bar::render_info_line(frame, plan.areas[3], app);
        status_bar::render_separator(frame, plan.areas[4]);
        input::render_input(frame, plan.areas[5], app);
        status_bar::render_separator(frame, plan.areas[6]);
        status_bar::render_blank_line(frame, plan.areas[7], app);
    } else {
        if plan.show_turn_status {
            turn_status::render_turn_status(frame, plan.areas[0], app);
        }
        pending::render_pending_inputs(frame, plan.areas[1], app);
        status_bar::render_info_line(frame, plan.areas[2], app);
        status_bar::render_separator(frame, plan.areas[3]);
        input::render_input(frame, plan.areas[4], app);
        status_bar::render_separator(frame, plan.areas[5]);
        status_bar::render_blank_line(frame, plan.areas[6], app);
    }
}
