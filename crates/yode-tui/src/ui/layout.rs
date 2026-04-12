use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::App;

pub struct MainLayoutPlan {
    pub areas: Vec<Rect>,
    pub show_turn_status: bool,
    pub show_completion: bool,
}

pub fn build_main_layout(area: Rect, app: &App) -> MainLayoutPlan {
    let term_width = area.width;
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

    let status_area_height = if completion_height > 0 {
        0
    } else if status_height_raw > 0 {
        3
    } else {
        0
    };

    let constraints = if completion_height > 0 {
        vec![
            Constraint::Length(completion_height),
            Constraint::Length(pending_height),
            Constraint::Length(1),
            Constraint::Length(input_height),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ]
    } else {
        vec![
            Constraint::Length(status_area_height),
            Constraint::Length(pending_height),
            Constraint::Length(1),
            Constraint::Length(input_height),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ]
    };

    MainLayoutPlan {
        areas: Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(area)
            .to_vec(),
        show_turn_status: status_area_height > 0,
        show_completion: completion_height > 0,
    }
}
