use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::App;

pub struct MainLayoutPlan {
    pub areas: Vec<Rect>,
    pub show_turn_status: bool,
    pub show_completion: bool,
}

pub(crate) fn status_area_height(app: &App, completion_height: u16) -> u16 {
    if completion_height > 0 {
        0
    } else if app.turn_status.is_visible() {
        3
    } else {
        0
    }
}

pub fn build_main_layout(area: Rect, app: &App) -> MainLayoutPlan {
    let term_width = area.width;
    let visual_lines = app.input.visual_line_count(term_width) as u16;
    let input_height = visual_lines.clamp(1, 5);
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

    let status_area_height = status_area_height(app, completion_height);

    let constraints = if completion_height > 0 {
        vec![
            Constraint::Length(completion_height),
            Constraint::Length(pending_height),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(input_height),
            Constraint::Length(1),
            Constraint::Length(1),
        ]
    } else {
        vec![
            Constraint::Length(status_area_height),
            Constraint::Length(pending_height),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(input_height),
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use yode_llm::registry::ProviderRegistry;
    use yode_tools::registry::ToolRegistry;

    use crate::app::App;

    use super::build_main_layout;

    fn test_app() -> App {
        App::new(
            "test-model".to_string(),
            "session-1234".to_string(),
            "/tmp".to_string(),
            "test".to_string(),
            Vec::new(),
            HashMap::new(),
            Arc::new(ProviderRegistry::new()),
            Arc::new(ToolRegistry::new()),
        )
    }

    #[test]
    fn streaming_preview_does_not_expand_status_height() {
        let mut app = test_app();
        app.turn_status = crate::app::TurnStatus::Working { verb: "Thinking" };
        app.streaming_markdown_preview = vec![
            ratatui::text::Line::from("a"),
            ratatui::text::Line::from("b"),
        ];
        let plan = build_main_layout(ratatui::layout::Rect::new(0, 0, 80, 20), &app);
        assert_eq!(plan.areas[0].height, 3);
        assert!(plan.show_turn_status);
    }

    #[test]
    fn empty_streaming_preview_keeps_compact_status_height() {
        let mut app = test_app();
        app.turn_status = crate::app::TurnStatus::Working { verb: "Thinking" };
        let plan = build_main_layout(ratatui::layout::Rect::new(0, 0, 80, 20), &app);
        assert_eq!(plan.areas[0].height, 3);
    }

    #[test]
    fn done_status_keeps_status_area_visible() {
        let mut app = test_app();
        app.turn_status = crate::app::TurnStatus::Done {
            elapsed: std::time::Duration::from_secs(2),
            tools: 3,
        };
        let plan = build_main_layout(ratatui::layout::Rect::new(0, 0, 80, 20), &app);
        assert_eq!(plan.areas[0].height, 3);
        assert!(plan.show_turn_status);
    }

    #[test]
    fn info_line_sits_above_input_area() {
        let mut app = test_app();
        app.input.lines = vec!["line 1".to_string(), "line 2".to_string()];

        let plan = build_main_layout(ratatui::layout::Rect::new(0, 0, 80, 20), &app);

        assert_eq!(plan.areas[2].height, 1);
        assert_eq!(plan.areas[3].height, 1);
        assert_eq!(plan.areas[4].height, 2);
        assert!(plan.areas[2].y < plan.areas[4].y);
    }
}
