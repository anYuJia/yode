use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use super::panels::{
    button_row_line, keyhint_bar_line, panel_rect_for_density, preview_empty_state,
    search_prompt_label, section_title_line, PanelFocusState,
};
use super::palette::{MUTED, PANEL_ACCENT, SELECT_ACCENT};
use super::responsive::density_from_width;

/// Render inline confirmation selector across 4 viewport lines.
pub fn render_inline_confirm(frame: &mut Frame, _chunks: &[Rect], app: &App) {
    let Some(ref confirm) = app.pending_confirmation else {
        return;
    };

    let args_display = format_tool_args_short(&confirm.name, &confirm.arguments);
    let options = vec![
        "Yes".to_string(),
        format!("Always allow {}", confirm.name),
        "No".to_string(),
    ];
    let selected = options
        .get(app.confirm_selected)
        .cloned()
        .unwrap_or_else(|| "Yes".to_string());

    let density = density_from_width(frame.area().width, 68, 96);
    let panel_area = panel_rect_for_density(frame.area(), density, 84, 4);
    let inner = [
        Rect::new(panel_area.x, panel_area.y, panel_area.width, 1),
        Rect::new(panel_area.x, panel_area.y + 1, panel_area.width, 1),
        Rect::new(panel_area.x, panel_area.y + 2, panel_area.width, 1),
        Rect::new(panel_area.x, panel_area.y + 3, panel_area.width, 1),
    ];

    frame.render_widget(
        Paragraph::new(format!(
            "  {} · {}",
            format!("Allow {}({})?", capitalize(&confirm.name), args_display),
            search_prompt_label(&selected),
        )),
        inner[0],
    );
    frame.render_widget(Paragraph::new(section_title_line("Confirm", PANEL_ACCENT)), inner[1]);
    frame.render_widget(
        Paragraph::new(button_row_line(&options, app.confirm_selected, SELECT_ACCENT, MUTED)),
        inner[2],
    );
    frame.render_widget(
        Paragraph::new(keyhint_bar_line(
            &["y/n/a", "↑↓ move", "Enter confirm"],
            PanelFocusState::Primary,
            PANEL_ACCENT,
            Color::Indexed(240),
        )),
        inner[3],
    );
    let _ = preview_empty_state("Confirm");
}

/// Short summary of tool arguments for the inline header.
fn format_tool_args_short(tool_name: &str, args_json: &str) -> String {
    let parsed: serde_json::Value = match serde_json::from_str(args_json) {
        Ok(v) => v,
        Err(_) => return "…".to_string(),
    };

    match tool_name {
        "bash" => {
            let cmd = parsed["command"].as_str().unwrap_or("???");
            truncate_str(cmd, 60)
        }
        "edit_file" | "write_file" | "read_file" => {
            parsed["file_path"].as_str().unwrap_or("???").to_string()
        }
        _ => {
            if let Some(obj) = parsed.as_object() {
                for key in &["command", "path", "file_path", "query", "pattern"] {
                    if let Some(val) = obj.get(*key).and_then(|v| v.as_str()) {
                        return truncate_str(val, 60);
                    }
                }
            }
            "…".to_string()
        }
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}
