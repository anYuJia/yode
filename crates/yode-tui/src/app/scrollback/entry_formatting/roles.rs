use std::time::Duration;

use ratatui::style::{Color, Modifier};

use crate::app::rendering::{
    capitalize, highlight_code_line, is_code_block_line, markdown_to_plain, truncate_line,
};
use crate::app::{ChatEntry, ChatRole};

pub(crate) fn format_entry_as_strings(
    entry: &ChatEntry,
    all_entries: &[ChatEntry],
    index: usize,
) -> Vec<(String, ratatui::style::Style)> {
    let mut result: Vec<(String, ratatui::style::Style)> = Vec::new();
    let dim = ratatui::style::Style::default().fg(Color::Gray);
    let accent = ratatui::style::Style::default().fg(Color::LightMagenta);
    let cyan = ratatui::style::Style::default().fg(Color::Indexed(51));
    let white = ratatui::style::Style::default().fg(Color::Indexed(231));
    let red = ratatui::style::Style::default().fg(Color::LightRed);

    match &entry.role {
        ChatRole::User => render_user(entry, &mut result, cyan),
        ChatRole::Assistant => render_assistant(entry, &mut result, dim, white),
        ChatRole::ToolCall { id: tid, name } => {
            render_tool_call(entry, all_entries, index, tid, name, &mut result, dim, accent, red)
        }
        ChatRole::ToolResult { id: rid, .. } => {
            let has_preceding = index > 0
                && all_entries[..index].iter().rev().any(
                    |e| matches!(&e.role, ChatRole::ToolCall { id: ref tid, .. } if tid == rid),
                );
            if !has_preceding {
                result.push((
                    format!("  ⎿ {}", entry.content.lines().next().unwrap_or("")),
                    dim,
                ));
            }
        }
        ChatRole::Error => {
            let err_style = ratatui::style::Style::default()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD);
            result.push(("╭─ Error ──────────────────────────".to_string(), err_style));
            for line in entry.content.lines() {
                result.push((format!("│ {}", line), red));
            }
            result.push(("╰──────────────────────────────────".to_string(), err_style));
        }
        ChatRole::System => {
            if entry.content.is_empty() {
                result.push((String::new(), dim));
            } else {
                for line in entry.content.lines() {
                    result.push((format!("  {}", line), dim));
                }
            }
        }
        ChatRole::SubAgentCall { description } => {
            render_subagent_call(description, all_entries, index, &mut result, dim, accent);
        }
        ChatRole::SubAgentToolCall { .. } => {}
        ChatRole::SubAgentResult => {}
        ChatRole::AskUser { .. } => {}
    }
    result
}

fn render_user(
    entry: &ChatEntry,
    result: &mut Vec<(String, ratatui::style::Style)>,
    cyan: ratatui::style::Style,
) {
    let mut first = true;
    for line in entry.content.lines() {
        if first {
            result.push((format!("> {}", line), cyan.add_modifier(Modifier::BOLD)));
            first = false;
        } else {
            result.push((format!("  {}", line), cyan));
        }
    }
    if first {
        result.push(("> ".to_string(), cyan.add_modifier(Modifier::BOLD)));
    }
}

fn render_assistant(
    entry: &ChatEntry,
    result: &mut Vec<(String, ratatui::style::Style)>,
    dim: ratatui::style::Style,
    white: ratatui::style::Style,
) {
    result.push((String::new(), dim));
    let processed = markdown_to_plain(&entry.content);
    if processed.trim().is_empty() {
        return;
    }
    let mut first = true;
    for line in processed.lines() {
        if line.trim().is_empty() {
            result.push((String::new(), dim));
            continue;
        }
        if first {
            result.push((format!("⏺ {}", line), white));
            first = false;
        } else if is_code_block_line(&line) {
            let highlighted = highlight_code_line(&line);
            result.push((format!("  {}", highlighted), ratatui::style::Style::default()));
        } else {
            result.push((format!("  {}", line), white));
        }
    }
}

fn render_tool_call(
    entry: &ChatEntry,
    all_entries: &[ChatEntry],
    index: usize,
    tool_id: &str,
    name: &str,
    result: &mut Vec<(String, ratatui::style::Style)>,
    dim: ratatui::style::Style,
    accent: ratatui::style::Style,
    red: ratatui::style::Style,
) {
    let args: serde_json::Value = serde_json::from_str(&entry.content).unwrap_or_default();
    let tool_result = all_entries[index + 1..].iter().find(
        |e| matches!(&e.role, ChatRole::ToolResult { id: ref eid, .. } if eid == tool_id),
    );

    let timing = tool_result
        .and_then(|r| r.duration)
        .map(format_timing)
        .unwrap_or_default();

    let green = ratatui::style::Style::default().fg(Color::LightGreen);
    let red_dim = ratatui::style::Style::default().fg(Color::LightRed);

    if name == "edit_file" {
        render_edit_file(args, &timing, result, dim, accent, green, red_dim);
    } else if name == "read_file" {
        let display_path = display_file_path(args["file_path"].as_str().unwrap_or("???"));
        result.push((format!("⏺ Read({}){}", display_path, timing), accent));
    } else if name == "write_file" {
        render_write_file(args, &timing, result, dim, accent, green);
    } else {
        let summary = tool_summary_str(name, &args);
        result.push((format!("⏺ {}({}){}", capitalize(name), summary, timing), accent));

        if let Some(res) = tool_result {
            let max_lines = 3;
            let max_line_chars = crossterm::terminal::size()
                .map(|(width, _)| (width as usize).saturating_sub(10))
                .unwrap_or(120);
            for (line_index, line) in res.content.lines().enumerate() {
                if line_index >= max_lines {
                    result.push((
                        format!(
                            "     … +{} lines (ctrl+o to expand)",
                            res.content.lines().count() - max_lines
                        ),
                        dim,
                    ));
                    break;
                }
                let prefix = if line_index == 0 { "  ⎿  " } else { "     " };
                let style =
                    if matches!(res.role, ChatRole::ToolResult { is_error, .. } if is_error) {
                        red
                    } else {
                        dim
                    };
                let display = truncate_line(line, max_line_chars);
                result.push((format!("{}{}", prefix, display), style));
            }
        }
    }
}

fn render_edit_file(
    args: serde_json::Value,
    timing: &str,
    result: &mut Vec<(String, ratatui::style::Style)>,
    dim: ratatui::style::Style,
    accent: ratatui::style::Style,
    green: ratatui::style::Style,
    red_dim: ratatui::style::Style,
) {
    let display_path = display_file_path(args["file_path"].as_str().unwrap_or("???"));
    let old_lines: Vec<&str> = args["old_string"].as_str().unwrap_or("").lines().collect();
    let new_lines: Vec<&str> = args["new_string"].as_str().unwrap_or("").lines().collect();
    let added = new_lines.len();
    let removed = old_lines.len();
    let summary = if added > 0 && removed > 0 {
        format!("Added {} lines, removed {} lines", added, removed)
    } else if added > 0 {
        format!("Added {} lines", added)
    } else {
        format!("Removed {} lines", removed)
    };

    result.push((format!("⏺ Update({}){}", display_path, timing), accent));
    result.push((format!("  ⎿  {}", summary), dim));

    let max_diff = 6;
    let mut shown = 0;
    let total = old_lines.len() + new_lines.len();
    for line in &old_lines {
        if shown >= max_diff {
            result.push((
                format!("     … +{} lines (ctrl+o to expand)", total - shown),
                dim,
            ));
            break;
        }
        result.push((format!("     - {}", line), red_dim));
        shown += 1;
    }
    if shown < max_diff {
        for line in &new_lines {
            if shown >= max_diff {
                result.push((
                    format!("     … +{} lines (ctrl+o to expand)", total - shown),
                    dim,
                ));
                break;
            }
            result.push((format!("     + {}", line), green));
            shown += 1;
        }
    }
}

fn render_write_file(
    args: serde_json::Value,
    timing: &str,
    result: &mut Vec<(String, ratatui::style::Style)>,
    dim: ratatui::style::Style,
    accent: ratatui::style::Style,
    green: ratatui::style::Style,
) {
    let display_path = display_file_path(args["file_path"].as_str().unwrap_or("???"));
    let content = args["content"].as_str().unwrap_or("");
    let total_lines = content.lines().count();
    result.push((format!("⏺ Write({}){}", display_path, timing), accent));
    result.push((format!("  ⎿  {} lines written", total_lines), dim));
    let max_preview = 3;
    for (index, line) in content.lines().enumerate() {
        if index >= max_preview {
            result.push((
                format!("     … +{} lines (ctrl+o to expand)", total_lines - max_preview),
                dim,
            ));
            break;
        }
        result.push((format!("     + {}", line), green));
    }
}

fn render_subagent_call(
    description: &str,
    all_entries: &[ChatEntry],
    index: usize,
    result: &mut Vec<(String, ratatui::style::Style)>,
    dim: ratatui::style::Style,
    accent: ratatui::style::Style,
) {
    let mut sub_tools: Vec<String> = Vec::new();
    let mut agent_duration: Option<Duration> = None;
    for entry in &all_entries[index + 1..] {
        match &entry.role {
            ChatRole::SubAgentToolCall { name } => sub_tools.push(name.clone()),
            ChatRole::SubAgentResult => {
                agent_duration = entry.duration;
                break;
            }
            _ => break,
        }
    }

    let agent_type = if description.to_lowercase().contains("explore") {
        "Explore"
    } else if description.to_lowercase().contains("plan") {
        "Plan"
    } else {
        "Agent"
    };

    let timing = agent_duration
        .map(|duration| format!(" ── {}", crate::app::format_duration(duration)))
        .unwrap_or_default();

    result.push((format!("⏺ {}({}){}", agent_type, description, timing), accent));

    let max_show = 3;
    let total = sub_tools.len();
    for (index, tool_name) in sub_tools.iter().enumerate() {
        if index >= max_show {
            result.push((
                format!("     … +{} more tool uses (ctrl+o to expand)", total - max_show),
                dim,
            ));
            break;
        }
        let prefix = if index == 0 { "  ⎿  " } else { "     " };
        result.push((format!("{}{}(…)", prefix, capitalize(tool_name)), dim));
    }
    if total == 0 {
        result.push(("  ⎿  (no tool calls)".to_string(), dim));
    }
}

fn format_timing(duration: Duration) -> String {
    if duration.as_secs() >= 1 {
        format!(" ── {:.1}s", duration.as_secs_f64())
    } else {
        format!(" ── {}ms", duration.as_millis())
    }
}

fn display_file_path(file_path: &str) -> &str {
    file_path
        .strip_prefix(&format!(
            "{}/",
            std::env::current_dir()
                .map(|path| path.display().to_string())
                .unwrap_or_default()
        ))
        .unwrap_or(file_path)
}

fn tool_summary_str(name: &str, args: &serde_json::Value) -> String {
    match name {
        "bash" => args["command"].as_str().unwrap_or("???").to_string(),
        "write_file" | "read_file" => args["file_path"].as_str().unwrap_or("???").to_string(),
        "edit_file" => args["file_path"].as_str().unwrap_or("???").to_string(),
        "glob" => args["pattern"].as_str().unwrap_or("???").to_string(),
        "grep" => args["pattern"].as_str().unwrap_or("???").to_string(),
        "agent" => args["description"].as_str().unwrap_or("???").to_string(),
        "memory" => {
            let action = args["action"].as_str().unwrap_or("???");
            let mem_name = args["name"].as_str().unwrap_or("");
            if mem_name.is_empty() {
                action.to_string()
            } else {
                format!("{} {}", action, mem_name)
            }
        }
        "cron" => args["action"].as_str().unwrap_or("???").to_string(),
        "lsp" => {
            let operation = args["operation"].as_str().unwrap_or("???");
            let file = args["filePath"].as_str().unwrap_or("");
            if file.is_empty() {
                operation.to_string()
            } else {
                format!("{} {}", operation, file)
            }
        }
        "enter_worktree" => args["name"].as_str().unwrap_or("").to_string(),
        "notebook_edit" => args["notebook_path"].as_str().unwrap_or("???").to_string(),
        _ => {
            if let Some(object) = args.as_object() {
                for key in &[
                    "command",
                    "path",
                    "file_path",
                    "relative_path",
                    "query",
                    "pattern",
                    "url",
                    "name",
                ] {
                    if let Some(value) = object.get(*key).and_then(|value| value.as_str()) {
                        return value.to_string();
                    }
                }
                for value in object.values() {
                    if let Some(string) = value.as_str() {
                        if string.len() <= 80 {
                            return string.to_string();
                        }
                    }
                }
            }
            String::new()
        }
    }
}
