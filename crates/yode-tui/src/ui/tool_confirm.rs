use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::palette::{BORDER_MUTED, ERROR_COLOR, LIGHT, MUTED, PANEL_ACCENT, SELECT_BG};
use super::panels::preview_empty_state;
use crate::ui::chat::render_markdown_white_with_options;
use crate::app::App;
use crate::display_text::{compact_path_tail as compact_path, human_tool_display_name};
use crate::tool_grouping::describe_groupable_tool_call;

/// Render inline confirmation selector in a bottom-anchored panel.
pub const INLINE_CONFIRM_HEIGHT: u16 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfirmDensity {
    Default,
    Narrow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfirmRiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConfirmRisk {
    level: ConfirmRiskLevel,
    label: String,
}

pub fn render_inline_confirm(frame: &mut Frame, area: Rect, app: &App) {
    let Some(ref confirm) = app.pending_confirmation else {
        return;
    };

    let tool_label = tool_display_name(app, &confirm.name);
    let activity = tool_activity_summary(app, &confirm.name, &confirm.arguments);
    let title = confirmation_title(app, &confirm.name, &confirm.arguments);
    let risk = tool_risk_hint(app, &confirm.name);
    let preview = tool_preview_line(&confirm.name, &confirm.arguments);
    let density = confirm_density(panel_area_width(area, frame));
    let options = vec![
        "Allow once (default)".to_string(),
        tool_allow_option_label(&confirm.name, &confirm.arguments, &tool_label),
        "Deny".to_string(),
    ];
    let confirm_height = inline_confirm_height(density);
    let panel_area = if area.height >= confirm_height {
        area
    } else {
        let full = frame.area();
        let y = full.y + full.height.saturating_sub(confirm_height);
        Rect::new(full.x, y, full.width, confirm_height)
    };

    let separator = "─".repeat(panel_area.width.saturating_sub(2) as usize);
    let truncate_width = match density {
        ConfirmDensity::Default => 96,
        ConfirmDensity::Narrow => 68,
    };
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
        Line::from(Span::styled(separator, Style::default().fg(BORDER_MUTED))),
        Line::from(vec![Span::styled(
            format!(
                "  {}",
                confirmation_section_title(&confirm.name, &tool_label)
            ),
            Style::default().fg(LIGHT).add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            format!(
                "   {}",
                truncate_str(
                    &confirmation_primary_value(&confirm.name, &confirm.arguments),
                    truncate_width
                )
            ),
            Style::default().fg(LIGHT),
        )]),
    ];
    if activity.trim() != confirmation_primary_value(&confirm.name, &confirm.arguments).trim() {
        lines.push(Line::from(vec![Span::styled(
            format!("   {}", truncate_str(&activity, truncate_width)),
            Style::default().fg(MUTED),
        )]));
    }
    if let Some(risk) = risk {
        lines.push(Line::from(vec![Span::styled(
            format!("   Safety: {}", risk.label),
            risk_style(risk.level),
        )]));
    }
    if !preview.trim().is_empty()
        && !preview.contains(&confirmation_primary_value(
            &confirm.name,
            &confirm.arguments,
        ))
    {
        lines.push(Line::from(vec![Span::styled(
            format!("   {}", truncate_str(&preview, truncate_width)),
            Style::default().fg(MUTED),
        )]));
    }
    if let Some(url) = confirm_full_url_preview(&confirm.name, &confirm.arguments) {
        lines.extend(prefixed_markdown_lines("   url · ", &url, truncate_width));
    }
    lines.push(Line::from(vec![Span::styled(
        " This command requires approval",
        Style::default()
            .fg(ERROR_COLOR)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(vec![Span::styled(
        " Do you want to proceed?",
        Style::default().fg(LIGHT),
    )]));
    lines.extend(option_list_lines(&options, app.confirm_selected));
    lines.push(Line::from(vec![Span::styled(
        match density {
            ConfirmDensity::Default => " Esc cancel · Ctrl+O inspect · Tab amend · Ctrl+E explain",
            ConfirmDensity::Narrow => " Esc cancel · ^O inspect · Tab amend · ^E explain",
        },
        Style::default().fg(MUTED),
    )]));

    frame.render_widget(Paragraph::new(lines), panel_area);
    let _ = preview_empty_state("Confirm");
}

fn panel_area_width(area: Rect, frame: &Frame) -> u16 {
    if area.width > 0 {
        area.width
    } else {
        frame.area().width
    }
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
        "agent" | "send_message" | "team_create" => "Delegated action".to_string(),
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
        "agent" | "send_message" | "team_create" => parsed
            .get("description")
            .or_else(|| parsed.get("message"))
            .and_then(|value| value.as_str())
            .unwrap_or("delegated action")
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

    if matches!(tool_name, "bash" | "powershell") {
        return shell_command_activity_summary(parsed["command"].as_str().unwrap_or("command"));
    }

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
        "edit_file" | "write_file" => {
            format!(
                "Update {}",
                compact_path(parsed["file_path"].as_str().unwrap_or("file"))
            )
        }
        "agent" | "send_message" | "team_create" => {
            let desc = parsed
                .get("description")
                .or_else(|| parsed.get("message"))
                .and_then(|value| value.as_str())
                .unwrap_or("delegated action");
            format!("Delegate {}", truncate_str(desc, 60))
        }
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
    human_tool_display_name(tool_name)
}

fn inline_confirm_height(density: ConfirmDensity) -> u16 {
    match density {
        ConfirmDensity::Default => INLINE_CONFIRM_HEIGHT,
        ConfirmDensity::Narrow => 12,
    }
}

fn confirm_density(width: u16) -> ConfirmDensity {
    if width < 72 {
        ConfirmDensity::Narrow
    } else {
        ConfirmDensity::Default
    }
}

fn risk_style(level: ConfirmRiskLevel) -> Style {
    match level {
        ConfirmRiskLevel::Low => Style::default().fg(MUTED),
        ConfirmRiskLevel::Medium => Style::default().fg(Color::Yellow),
        ConfirmRiskLevel::High => Style::default().fg(ERROR_COLOR).add_modifier(Modifier::BOLD),
    }
}

fn tool_risk_hint(app: &App, tool_name: &str) -> Option<ConfirmRisk> {
    if let Some(tool) = app.tools.get(tool_name) {
        if tool.capabilities().read_only {
            return Some(ConfirmRisk {
                level: ConfirmRiskLevel::Low,
                label: "low · read-only access".to_string(),
            });
        }
    }

    let (level, label) = match tool_name {
        "edit_file" | "write_file" | "multi_edit" | "notebook_edit" => {
            (ConfirmRiskLevel::High, "high · file edits")
        }
        "bash" | "powershell" => (ConfirmRiskLevel::High, "high · shell execution"),
        "git_commit" => (ConfirmRiskLevel::High, "high · git write"),
        "web_search" | "web_fetch" | "web_browser" => {
            (ConfirmRiskLevel::Medium, "medium · external access")
        }
        "agent" | "send_message" | "team_create" => {
            (ConfirmRiskLevel::Medium, "medium · delegated action")
        }
        _ => (ConfirmRiskLevel::Medium, "medium · approval required"),
    };
    Some(ConfirmRisk {
        level,
        label: label.to_string(),
    })
}

fn tool_preview_line(tool_name: &str, args_json: &str) -> String {
    let parsed: serde_json::Value = match serde_json::from_str(args_json) {
        Ok(v) => v,
        Err(_) => return "raw arguments unavailable".to_string(),
    };

    match tool_name {
        "bash" | "powershell" => shell_command_preview(
            parsed
                .get("command")
                .and_then(|value| value.as_str())
                .unwrap_or("command"),
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

fn confirm_full_url_preview(tool_name: &str, args_json: &str) -> Option<String> {
    if tool_name != "web_fetch" {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(args_json)
        .ok()
        .and_then(|parsed| parsed.get("url").and_then(|value| value.as_str()).map(str::to_string))
}

fn prefixed_markdown_lines(prefix: &str, text: &str, max_width: usize) -> Vec<Line<'static>> {
    let rendered = render_markdown_white_with_options(text, Some(max_width), true);
    let mut lines = Vec::new();
    for (index, line) in rendered.into_iter().enumerate() {
        let prefix_text = if index == 0 { prefix } else { "         " };
        let mut spans = vec![Span::styled(prefix_text.to_string(), Style::default().fg(MUTED))];
        spans.extend(line.spans);
        lines.push(Line::from(spans));
    }
    lines
}

fn shell_command_preview(command: &str) -> String {
    let lines = command.lines().collect::<Vec<_>>();
    let head = lines.first().copied().unwrap_or("command");
    if lines.len() > 1 {
        format!("command · {} ↳ +{} more lines", head, lines.len() - 1)
    } else {
        format!("command · {}", head)
    }
}

fn shell_command_activity_summary(command: &str) -> String {
    let lines = command.lines().collect::<Vec<_>>();
    let head = lines.first().copied().unwrap_or("command");
    if lines.len() > 1 {
        format!("Run {} ↳ +{} more lines", truncate_str(head, 60), lines.len() - 1)
    } else {
        format!("Run {}", truncate_str(head, 60))
    }
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
        confirmation_primary_value, confirmation_section_title, confirmation_title,
        confirm_density, inline_confirm_height, option_list_lines, tool_activity_summary,
        tool_allow_option_label, tool_display_name, tool_preview_line, tool_risk_hint,
        ConfirmDensity, ConfirmRiskLevel,
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
    fn confirmation_activity_summary_folds_multiline_shell_commands() {
        let app = test_app();
        let summary = tool_activity_summary(
            &app,
            "bash",
            r#"{"command":"python main.py\npytest -q\ncargo test"}"#,
        );
        assert_eq!(summary, "Run python main.py ↳ +2 more lines");
    }

    #[test]
    fn confirmation_uses_user_facing_tool_name_and_risk_hint() {
        let app = test_app();
        assert_eq!(tool_display_name(&app, "web_search"), "Web Search");
        assert_eq!(
            tool_risk_hint(&app, "edit_file")
                .map(|risk| (risk.level, risk.label)),
            Some((ConfirmRiskLevel::High, "high · file edits".to_string()))
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
    fn confirmation_primary_value_handles_delegated_actions() {
        let value = confirmation_primary_value(
            "agent",
            r#"{"description":"review current diff"}"#,
        );
        assert_eq!(value, "review current diff");
        assert_eq!(confirmation_section_title("agent", "Agent"), "Delegated action");
    }

    #[test]
    fn confirm_verbs_stay_consistent() {
        let lines = option_list_lines(
            &[
                "Allow once (default)".to_string(),
                "Always allow: Bash".to_string(),
                "Deny".to_string(),
            ],
            0,
        );
        let rendered = lines.iter().map(|line| line.to_string()).collect::<Vec<_>>();
        assert!(rendered[0].contains("Allow once"));
        assert!(rendered[1].contains("Always allow"));
        assert!(rendered[2].contains("Deny"));
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
    fn confirmation_full_url_preview_keeps_hyperlinked_url() {
        let lines = super::prefixed_markdown_lines(
            "   url · ",
            "https://docs.rs/ratatui/latest/ratatui/widgets/struct.Paragraph.html",
            96,
        );
        assert!(lines
            .iter()
            .any(|line| line.to_string().contains("\u{1b}]8;;https://docs.rs/ratatui/latest/ratatui/widgets/struct.Paragraph.html")));
    }

    #[test]
    fn confirmation_preview_line_folds_multiline_shell_commands() {
        let preview = tool_preview_line(
            "bash",
            r#"{"command":"python main.py\npytest -q\ncargo test"}"#,
        );
        assert_eq!(preview, "command · python main.py ↳ +2 more lines");
    }

    #[test]
    fn confirmation_options_render_as_vertical_selection() {
        let lines = option_list_lines(
            &[
                "Allow once (default)".to_string(),
                "Always allow: python *".to_string(),
                "Deny".to_string(),
            ],
            1,
        );
        assert!(lines[1].to_string().contains("❯ 2. Always allow: python *"));
        assert!(lines[0].to_string().contains("1. Allow once (default)"));
    }

    #[test]
    fn bash_allow_option_uses_command_prefix_pattern() {
        let label = tool_allow_option_label("bash", r#"{"command":"python main.py"}"#, "Bash");
        assert_eq!(label, "Always allow: python *");
    }

    #[test]
    fn confirmation_density_switches_on_narrow_widths() {
        assert_eq!(confirm_density(80), ConfirmDensity::Default);
        assert_eq!(confirm_density(60), ConfirmDensity::Narrow);
        assert_eq!(inline_confirm_height(ConfirmDensity::Narrow), 12);
    }

    #[test]
    fn print_confirm_regression_snapshot() {
        let app = test_app();
        println!("# Confirm Regression Snapshot\n");

        for (label, tool_name, args) in [
            ("Shell", "bash", r#"{"command":"python main.py\npytest -q\ncargo test"}"#),
            ("Network", "web_fetch", r#"{"url":"https://docs.rs/ratatui/latest/ratatui/widgets/struct.Paragraph.html"}"#),
            ("Write", "edit_file", r#"{"file_path":"/tmp/src/main.rs"}"#),
        ] {
            println!("## {}\n", label);
            let tool_label = tool_display_name(&app, tool_name);
            println!("title: {}", confirmation_title(&app, tool_name, args));
            println!("activity: {}", tool_activity_summary(&app, tool_name, args));
            if let Some(risk) = tool_risk_hint(&app, tool_name) {
                println!("risk: {}", risk.label);
            }
            println!("preview: {}", tool_preview_line(tool_name, args));
            println!(
                "options: Allow once | {} | Deny\n",
                tool_allow_option_label(tool_name, args, &tool_label)
            );
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
