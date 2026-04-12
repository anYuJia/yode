use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use super::panels::{button_row_line, footer_hint_line, inspector_header_lines, section_title_line};
use super::palette::{LIGHT, MUTED, PANEL_ACCENT, SELECT_ACCENT};

/// Render inline confirmation selector across 4 viewport lines.
pub fn render_inline_confirm(frame: &mut Frame, chunks: &[Rect], app: &App) {
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

    let header = inspector_header_lines(
        &format!("Allow {}({})?", capitalize(&confirm.name), args_display),
        Some(&format!("Selected: {}", selected)),
        PANEL_ACCENT,
        LIGHT,
        MUTED,
    );
    frame.render_widget(Paragraph::new(header[0].clone()), chunks[0]);
    frame.render_widget(Paragraph::new(section_title_line("Confirm", PANEL_ACCENT)), chunks[1]);
    frame.render_widget(
        Paragraph::new(button_row_line(&options, app.confirm_selected, SELECT_ACCENT, MUTED)),
        chunks[2],
    );
    frame.render_widget(
        Paragraph::new(footer_hint_line(&["y/n/a", "↑↓ move", "Enter confirm"], Color::Indexed(240))),
        chunks[3],
    );
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
