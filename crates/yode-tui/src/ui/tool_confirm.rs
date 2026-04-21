use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::tool_grouping::describe_groupable_tool_call;
use super::panels::{
    button_row_line, keyhint_bar_line, panel_rect_for_density, preview_empty_state,
    search_prompt_label, PanelFocusState,
};
use super::palette::{MUTED, PANEL_ACCENT, SELECT_BG};
use super::responsive::density_from_width;

/// Render inline confirmation selector across 5 viewport lines.
pub fn render_inline_confirm(frame: &mut Frame, _chunks: &[Rect], app: &App) {
    let Some(ref confirm) = app.pending_confirmation else {
        return;
    };

    let tool_label = tool_display_name(app, &confirm.name);
    let activity = tool_activity_summary(app, &confirm.name, &confirm.arguments);
    let risk_hint = tool_risk_hint(app, &confirm.name);
    let preview = tool_preview_line(&confirm.name, &confirm.arguments);
    let options = vec![
        "Yes".to_string(),
        format!("Always allow {}", tool_label),
        "No".to_string(),
    ];
    let selected = options
        .get(app.confirm_selected)
        .cloned()
        .unwrap_or_else(|| "Yes".to_string());

    let density = density_from_width(frame.area().width, 68, 96);
    let panel_area = panel_rect_for_density(frame.area(), density, 96, 5);
    let inner = [
        Rect::new(panel_area.x, panel_area.y, panel_area.width, 1),
        Rect::new(panel_area.x, panel_area.y + 1, panel_area.width, 1),
        Rect::new(panel_area.x, panel_area.y + 2, panel_area.width, 1),
        Rect::new(panel_area.x, panel_area.y + 3, panel_area.width, 1),
        Rect::new(panel_area.x, panel_area.y + 4, panel_area.width, 1),
    ];

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                format!("Allow {}?", tool_label),
                Style::default().fg(Color::White),
            ),
            Span::styled(" · ", Style::default().fg(MUTED)),
            Span::styled(search_prompt_label(&selected), Style::default().fg(MUTED)),
        ])),
        inner[0],
    );
    let detail = match risk_hint {
        Some(risk) => format!("  {} · {}", truncate_str(&activity, 72), risk),
        None => format!("  {}", truncate_str(&activity, 72)),
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            detail,
            Style::default().fg(Color::Gray),
        )])),
        inner[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!("  {}", truncate_str(&preview, 84)),
            Style::default().fg(Color::DarkGray),
        )])),
        inner[2],
    );
    frame.render_widget(
        Paragraph::new(button_row_line(&options, app.confirm_selected, SELECT_BG, MUTED)),
        inner[3],
    );
    frame.render_widget(
        Paragraph::new(keyhint_bar_line(
            &["y/n/a", "↑↓ move", "Enter confirm", "Ctrl+O details"],
            PanelFocusState::Primary,
            PANEL_ACCENT,
            Color::Indexed(240),
        )),
        inner[4],
    );
    let _ = preview_empty_state("Confirm");
}

fn tool_activity_summary(app: &App, tool_name: &str, args_json: &str) -> String {
    let parsed: serde_json::Value = match serde_json::from_str(args_json) {
        Ok(v) => v,
        Err(_) => return "Pending tool execution".to_string(),
    };

    if let Some(description) = describe_groupable_tool_call(tool_name, &parsed, true) {
        return description;
    }

    if let Some(tool) = app.tools.get(tool_name) {
        let description = tool.activity_description(&parsed);
        if !description.trim().is_empty() {
            return description;
        }
    }

    match tool_name {
        "bash" | "powershell" => {
            let cmd = parsed["command"].as_str().unwrap_or("command");
            format!("Run {}", truncate_str(cmd, 60))
        }
        "edit_file" | "write_file" => format!(
            "Update {}",
            parsed["file_path"].as_str().unwrap_or("file")
        ),
        _ => "Pending tool execution".to_string(),
    }
}

fn tool_display_name(app: &App, tool_name: &str) -> String {
    if let Some(tool) = app.tools.get(tool_name) {
        let label = tool.user_facing_name();
        if !label.trim().is_empty() {
            return label.to_string();
        }
    }

    tool_name
        .split('_')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn tool_risk_hint(app: &App, tool_name: &str) -> Option<String> {
    if let Some(tool) = app.tools.get(tool_name) {
        if tool.capabilities().read_only {
            return Some("read-only".to_string());
        }
    }

    let hint = match tool_name {
        "edit_file" | "write_file" | "multi_edit" | "notebook_edit" => "changes files",
        "bash" | "powershell" => "shell access",
        "web_search" | "web_fetch" | "web_browser" => "network access",
        "git_commit" => "git write",
        "agent" | "send_message" | "team_create" => "agent action",
        _ => "needs approval",
    };
    Some(hint.to_string())
}

fn tool_preview_line(tool_name: &str, args_json: &str) -> String {
    let parsed: serde_json::Value = match serde_json::from_str(args_json) {
        Ok(v) => v,
        Err(_) => return "raw arguments unavailable".to_string(),
    };

    match tool_name {
        "bash" | "powershell" => format!(
            "command · {}",
            parsed
                .get("command")
                .and_then(|value| value.as_str())
                .unwrap_or("command")
        ),
        "read_file" | "write_file" | "edit_file" | "multi_edit" => format!(
            "path · {}",
            parsed
                .get("file_path")
                .and_then(|value| value.as_str())
                .unwrap_or("file")
        ),
        "web_search" => format!(
            "query · {}",
            parsed
                .get("query")
                .and_then(|value| value.as_str())
                .unwrap_or("query")
        ),
        "web_fetch" => format!(
            "url · {}",
            parsed
                .get("url")
                .and_then(|value| value.as_str())
                .unwrap_or("url")
        ),
        "lsp" => format!(
            "operation · {} @ {}",
            parsed
                .get("operation")
                .and_then(|value| value.as_str())
                .unwrap_or("query"),
            parsed
                .get("filePath")
                .and_then(|value| value.as_str())
                .unwrap_or("file")
        ),
        "batch" => format!(
            "parallel calls · {}",
            parsed
                .get("invocations")
                .and_then(|value| value.as_array())
                .map(|items| items.len())
                .unwrap_or(0)
        ),
        _ => parsed
            .as_object()
            .and_then(|object| {
                ["command", "path", "file_path", "query", "pattern", "url", "name"]
                    .iter()
                    .find_map(|key| object.get(*key).and_then(|value| value.as_str()))
            })
            .map(|value| format!("preview · {}", value))
            .unwrap_or_else(|| "preview unavailable".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use yode_llm::registry::ProviderRegistry;
    use yode_tools::builtin::{register_builtin_tools, register_skill_tool};
    use yode_tools::builtin::skill::SkillStore;
    use yode_tools::registry::ToolRegistry;
    use tokio::sync::Mutex;

    use crate::app::App;

    use super::{tool_activity_summary, tool_display_name, tool_preview_line, tool_risk_hint};

    fn test_app() -> App {
        let registry = Arc::new(ToolRegistry::new());
        register_builtin_tools(&registry);
        register_skill_tool(&registry, Arc::new(Mutex::new(SkillStore::new())));
        App::new(
            "test-model".to_string(),
            "session-1234".to_string(),
            "/tmp".to_string(),
            "test".to_string(),
            Vec::new(),
            HashMap::new(),
            Arc::new(ProviderRegistry::new()),
            registry,
        )
    }

    #[test]
    fn confirmation_uses_activity_summary_for_groupable_tools() {
        let app = test_app();
        let summary = tool_activity_summary(
            &app,
            "read_file",
            r#"{"file_path":"/tmp/src/main.rs"}"#,
        );
        assert_eq!(summary, "Reading .../src/main.rs");
    }

    #[test]
    fn confirmation_uses_user_facing_tool_name_and_risk_hint() {
        let app = test_app();
        assert_eq!(tool_display_name(&app, "web_search"), "Web Search");
        assert_eq!(tool_risk_hint(&app, "edit_file").as_deref(), Some("changes files"));
    }

    #[test]
    fn confirmation_preview_line_uses_key_fields() {
        let preview = tool_preview_line("bash", r#"{"command":"cargo test -p yode-tui"}"#);
        assert!(preview.contains("command · cargo test -p yode-tui"));
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}
