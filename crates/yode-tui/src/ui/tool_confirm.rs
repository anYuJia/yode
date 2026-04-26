use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::palette::{BORDER_MUTED, ERROR_COLOR, LIGHT, MUTED, PANEL_ACCENT, SELECT_BG};
use super::panels::preview_empty_state;
use crate::app::App;
use crate::tool_grouping::describe_groupable_tool_call;

/// Render inline confirmation selector in a bottom-anchored panel.
pub const INLINE_CONFIRM_HEIGHT: u16 = 14;

pub fn render_inline_confirm(frame: &mut Frame, area: Rect, app: &App) {
    let Some(ref confirm) = app.pending_confirmation else {
        return;
    };

    let tool_label = tool_display_name(app, &confirm.name);
    let activity = tool_activity_summary(app, &confirm.name, &confirm.arguments);
    let title = confirmation_title(app, &confirm.name, &confirm.arguments);
    let risk_hint = tool_risk_hint(app, &confirm.name);
    let preview = tool_preview_line(&confirm.name, &confirm.arguments);
    let options = vec![
        "Allow once".to_string(),
        tool_allow_option_label(&confirm.name, &confirm.arguments, &tool_label),
        "Deny".to_string(),
    ];
    let panel_area = if area.height >= INLINE_CONFIRM_HEIGHT {
        area
    } else {
        let full = frame.area();
        let y = full.y + full.height.saturating_sub(INLINE_CONFIRM_HEIGHT);
        Rect::new(full.x, y, full.width, INLINE_CONFIRM_HEIGHT)
    };

    let separator = "─".repeat(panel_area.width.saturating_sub(2) as usize);
    let mut lines = vec![
        Line::from(vec![
            Span::styled("⏺ ", Style::default().fg(PANEL_ACCENT)),
            Span::styled(
                title,
                Style::default().fg(LIGHT).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  ⎿  ", Style::default().fg(MUTED)),
            Span::styled("Running...", Style::default().fg(MUTED)),
        ]),
        Line::from(""),
        Line::from(Span::styled(separator, Style::default().fg(BORDER_MUTED))),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!(
                "  {}",
                confirmation_section_title(&confirm.name, &tool_label)
            ),
            Style::default().fg(LIGHT).add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!(
                "   {}",
                truncate_str(
                    &confirmation_primary_value(&confirm.name, &confirm.arguments),
                    96
                )
            ),
            Style::default().fg(LIGHT),
        )]),
    ];
    if activity.trim() != confirmation_primary_value(&confirm.name, &confirm.arguments).trim() {
        lines.push(Line::from(vec![Span::styled(
            format!("   {}", truncate_str(&activity, 96)),
            Style::default().fg(MUTED),
        )]));
    }
    if let Some(risk) = risk_hint {
        lines.push(Line::from(vec![Span::styled(
            format!("   Risk: {}", risk),
            Style::default().fg(MUTED),
        )]));
    }
    if !preview.trim().is_empty()
        && !preview.contains(&confirmation_primary_value(
            &confirm.name,
            &confirm.arguments,
        ))
    {
        lines.push(Line::from(vec![Span::styled(
            format!("   {}", truncate_str(&preview, 96)),
            Style::default().fg(MUTED),
        )]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        " This command requires approval",
        Style::default()
            .fg(ERROR_COLOR)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        " Do you want to proceed?",
        Style::default().fg(LIGHT),
    )]));
    lines.extend(option_list_lines(&options, app.confirm_selected));
    lines.push(Line::from(vec![Span::styled(
        " Esc to cancel · Tab to amend · Ctrl+E to explain",
        Style::default().fg(MUTED),
    )]));

    frame.render_widget(Paragraph::new(lines), panel_area);
    let _ = preview_empty_state("Confirm");
}

fn option_list_lines(options: &[String], selected: usize) -> Vec<Line<'static>> {
    options
        .iter()
        .enumerate()
        .map(|(index, label)| {
            let prefix = if index == selected { "  ❯ " } else { "    " };
            let style = if index == selected {
                Style::default()
                    .fg(Color::White)
                    .bg(SELECT_BG)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(MUTED)
            };
            Line::from(vec![Span::styled(
                format!("{}{}. {}", prefix, index + 1, label),
                style,
            )])
        })
        .collect()
}

fn confirmation_title(app: &App, tool_name: &str, args_json: &str) -> String {
    let label = tool_display_name(app, tool_name);
    format!(
        "{}({})",
        label,
        truncate_str(&confirmation_primary_value(tool_name, args_json), 56)
    )
}

fn confirmation_section_title(tool_name: &str, tool_label: &str) -> String {
    match tool_name {
        "bash" => "Bash command".to_string(),
        "powershell" => "PowerShell command".to_string(),
        _ => format!("{} request", tool_label),
    }
}

fn confirmation_primary_value(tool_name: &str, args_json: &str) -> String {
    let parsed: serde_json::Value = match serde_json::from_str(args_json) {
        Ok(v) => v,
        Err(_) => return "pending tool execution".to_string(),
    };

    match tool_name {
        "bash" | "powershell" => parsed
            .get("command")
            .and_then(|value| value.as_str())
            .unwrap_or("command")
            .lines()
            .next()
            .unwrap_or("command")
            .to_string(),
        "read_file" | "write_file" | "edit_file" | "multi_edit" => compact_path(
            parsed
                .get("file_path")
                .and_then(|value| value.as_str())
                .unwrap_or("file"),
        ),
        "lsp" => format!(
            "{} @ {}",
            parsed
                .get("operation")
                .and_then(|value| value.as_str())
                .unwrap_or("operation"),
            compact_path(
                parsed
                    .get("filePath")
                    .and_then(|value| value.as_str())
                    .unwrap_or("file")
            )
        ),
        "web_search" => parsed
            .get("query")
            .and_then(|value| value.as_str())
            .unwrap_or("query")
            .to_string(),
        "web_fetch" => parsed
            .get("url")
            .and_then(|value| value.as_str())
            .unwrap_or("url")
            .to_string(),
        _ => tool_preview_line(tool_name, args_json),
    }
}

fn tool_allow_option_label(tool_name: &str, args_json: &str, tool_label: &str) -> String {
    if matches!(tool_name, "bash" | "powershell") {
        let parsed: serde_json::Value = match serde_json::from_str(args_json) {
            Ok(v) => v,
            Err(_) => {
                return format!("Always allow: {}", tool_label);
            }
        };
        if let Some(command) = parsed.get("command").and_then(|value| value.as_str()) {
            if let Some(first) = command.split_whitespace().next() {
                return format!("Always allow: {} *", first);
            }
        }
    }
    format!("Always allow: {}", tool_label)
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
        "edit_file" | "write_file" => {
            format!(
                "Update {}",
                compact_path(parsed["file_path"].as_str().unwrap_or("file"))
            )
        }
        _ => "Pending tool execution".to_string(),
    }
}

fn compact_path(path: &str) -> String {
    let parts: Vec<&str> = path.rsplitn(3, '/').collect();
    if parts.len() >= 3 {
        format!(".../{}/{}", parts[1], parts[0])
    } else {
        path.to_string()
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
            compact_path(
                parsed
                    .get("file_path")
                    .and_then(|value| value.as_str())
                    .unwrap_or("file")
            )
        ),
        "web_search" => format!(
            "query · {}",
            parsed
                .get("query")
                .and_then(|value| value.as_str())
                .unwrap_or("query")
        ),
        "web_fetch" => format!(
            "host · {}",
            compact_url_host(
                parsed
                    .get("url")
                    .and_then(|value| value.as_str())
                    .unwrap_or("url")
            )
        ),
        "lsp" => format!(
            "operation · {} @ {}",
            parsed
                .get("operation")
                .and_then(|value| value.as_str())
                .unwrap_or("query"),
            compact_path(
                parsed
                    .get("filePath")
                    .and_then(|value| value.as_str())
                    .unwrap_or("file")
            )
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
                [
                    "command",
                    "path",
                    "file_path",
                    "query",
                    "pattern",
                    "url",
                    "name",
                ]
                .iter()
                .find_map(|key| object.get(*key).and_then(|value| value.as_str()).map(|value| (*key, value)))
            })
            .map(|(key, value)| {
                let value = if matches!(key, "path" | "file_path") {
                    compact_path(value)
                } else {
                    value.to_string()
                };
                format!("preview · {}", value)
            })
            .unwrap_or_else(|| "preview unavailable".to_string()),
    }
}

fn compact_url_host(url: &str) -> String {
    let trimmed = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    trimmed
        .split(['/', '?', '#'])
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or(url)
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use tokio::sync::Mutex;
    use yode_llm::registry::ProviderRegistry;
    use yode_tools::builtin::skill::SkillStore;
    use yode_tools::builtin::{register_builtin_tools, register_skill_tool};
    use yode_tools::registry::ToolRegistry;

    use crate::app::App;

    use super::{
        confirmation_primary_value, option_list_lines, tool_activity_summary,
        tool_allow_option_label, tool_display_name, tool_preview_line, tool_risk_hint,
    };

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
        let summary =
            tool_activity_summary(&app, "read_file", r#"{"file_path":"/tmp/src/main.rs"}"#);
        assert_eq!(summary, "Reading .../src/main.rs");
    }

    #[test]
    fn confirmation_uses_user_facing_tool_name_and_risk_hint() {
        let app = test_app();
        assert_eq!(tool_display_name(&app, "web_search"), "Web Search");
        assert_eq!(
            tool_risk_hint(&app, "edit_file").as_deref(),
            Some("changes files")
        );
    }

    #[test]
    fn confirmation_preview_line_uses_key_fields() {
        let preview = tool_preview_line("bash", r#"{"command":"cargo test -p yode-tui"}"#);
        assert!(preview.contains("command · cargo test -p yode-tui"));
    }

    #[test]
    fn confirmation_preview_line_compacts_paths() {
        let preview = tool_preview_line("read_file", r#"{"file_path":"/tmp/src/main.rs"}"#);
        assert_eq!(preview, "path · .../src/main.rs");
    }

    #[test]
    fn confirmation_primary_value_compacts_file_paths() {
        let value = confirmation_primary_value("read_file", r#"{"file_path":"/tmp/src/main.rs"}"#);
        assert_eq!(value, ".../src/main.rs");
    }

    #[test]
    fn confirmation_primary_value_compacts_lsp_paths() {
        let value = confirmation_primary_value(
            "lsp",
            r#"{"operation":"hover","filePath":"/tmp/src/main.rs"}"#,
        );
        assert_eq!(value, "hover @ .../src/main.rs");
    }

    #[test]
    fn confirmation_preview_line_emphasizes_url_host() {
        let preview = tool_preview_line(
            "web_fetch",
            r#"{"url":"https://docs.rs/ratatui/latest/ratatui/widgets/struct.Paragraph.html"}"#,
        );
        assert_eq!(preview, "host · docs.rs");
    }

    #[test]
    fn confirmation_options_render_as_vertical_selection() {
        let lines = option_list_lines(
            &[
                "Allow once".to_string(),
                "Always allow: python *".to_string(),
                "Deny".to_string(),
            ],
            1,
        );
        assert!(lines[1].to_string().contains("❯ 2. Always allow: python *"));
        assert!(lines[0].to_string().contains("1. Allow once"));
    }

    #[test]
    fn bash_allow_option_uses_command_prefix_pattern() {
        let label = tool_allow_option_label("bash", r#"{"command":"python main.py"}"#, "Bash");
        assert_eq!(label, "Always allow: python *");
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}
