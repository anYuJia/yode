use std::time::Duration;

use ratatui::style::Color;

use crate::app::rendering::truncate_line;
use crate::app::{ChatEntry, ChatRole};
use crate::tool_grouping::{describe_tool_call, tool_batch_summary_text, ToolBatch};
use crate::tool_output_summary::{parse_shell_output_sections, summarize_tool_result, ToolSummaryTone};

pub(super) fn render_tool_call(
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
    let tool_result = all_entries[index + 1..]
        .iter()
        .find(|e| matches!(&e.role, ChatRole::ToolResult { id: ref eid, .. } if eid == tool_id));
    let is_error = tool_result
        .map(|r| matches!(r.role, ChatRole::ToolResult { is_error, .. } if is_error))
        .unwrap_or(false);
    let result_content = tool_result.map(|r| r.content.as_str()).unwrap_or("");
    let summary_result = summarize_tool_result(
        name,
        &args,
        tool_result.and_then(|entry| entry.tool_metadata.as_ref()),
        result_content,
        is_error,
    );

    let timing = tool_result
        .and_then(|r| r.duration)
        .map(format_timing)
        .unwrap_or_default();

    let green = ratatui::style::Style::default().fg(Color::LightGreen);
    let red_dim = ratatui::style::Style::default().fg(Color::LightRed);

    if name == "edit_file" {
        render_edit_file(args, &timing, result, dim, accent, green, red_dim);
    } else if name == "write_file" {
        render_write_file(args, &timing, result, dim, accent, green);
    } else {
        let summary = tool_summary_str(name, &args);
        let activity_title = describe_tool_call(name, &args, tool_result.is_none());
        let title = activity_title.unwrap_or_else(|| {
            format!("{}({})", tool_display_name(name), summary)
        });
        result.push((
            format!("⏺ {}{}", title, timing),
            if is_error { red } else { accent },
        ));

        if let Some(metadata) = tool_result.and_then(|entry| entry.tool_metadata.as_ref()) {
            render_metadata_hints(result, metadata, dim);
        }
        render_summary_lines(result, &summary_result.lines, dim, green, red, red_dim);

        if let Some(res) = tool_result {
            if summary_result.hide_body_by_default {
                return;
            }
            if matches!(name, "bash" | "powershell") {
                render_shell_output_lines(result, &res.content, dim, red);
            } else {
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
                    let style = if matches!(res.role, ChatRole::ToolResult { is_error, .. } if is_error)
                    {
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
}

fn render_metadata_hints(
    result: &mut Vec<(String, ratatui::style::Style)>,
    metadata: &serde_json::Value,
    dim: ratatui::style::Style,
) {
    if let Some(reason) = metadata
        .get("read_only_reason")
        .and_then(|value| value.as_str())
    {
        result.push((
            format!("  │ read-only: {}", reason),
            ratatui::style::Style::default().fg(Color::Cyan),
        ));
    } else if metadata
        .get("read_only")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        result.push((
            "  │ read-only command".to_string(),
            ratatui::style::Style::default().fg(Color::Cyan),
        ));
    }

    if let Some(warning) = metadata
        .get("destructive_warning")
        .and_then(|value| value.as_str())
    {
        result.push((
            format!("  │ warning: {}", warning),
            ratatui::style::Style::default().fg(Color::Yellow),
        ));
    }

    if let Some(suggestion) = metadata
        .get("rewrite_suggestion")
        .and_then(|value| value.as_str())
    {
        result.push((
            format!("  │ hint: {}", suggestion),
            ratatui::style::Style::default().fg(Color::Cyan),
        ));
    }

    if let Some(reason) = metadata
        .get("tool_runtime")
        .and_then(|value| value.get("truncation"))
        .and_then(|value| value.get("reason"))
        .and_then(|value| value.as_str())
    {
        result.push((format!("  │ truncated: {}", reason), dim));
    }
}

pub(super) fn render_grouped_tool_call(
    all_entries: &[ChatEntry],
    batch: &ToolBatch,
    result: &mut Vec<(String, ratatui::style::Style)>,
    dim: ratatui::style::Style,
    accent: ratatui::style::Style,
) {
    result.push((format!("⏺ {}", tool_batch_summary_text(batch)), accent));

    let max_items = 4;
    for (index, item) in batch.items.iter().take(max_items).enumerate() {
        let call = &all_entries[item.call_index];
        let args: serde_json::Value = serde_json::from_str(&call.content).unwrap_or_default();
        let tool_result = item
            .result_index
            .and_then(|result_index| all_entries.get(result_index));
        let is_error = tool_result
            .map(|r| matches!(r.role, ChatRole::ToolResult { is_error, .. } if is_error))
            .unwrap_or(false);
        let summary_result = summarize_tool_result(
            &item.tool_name,
            &args,
            tool_result.and_then(|entry| entry.tool_metadata.as_ref()),
            tool_result
                .map(|entry| entry.content.as_str())
                .unwrap_or(""),
            is_error,
        );
        let target = group_item_target(&item.tool_name, &args);
        let detail = summary_result
            .lines
            .first()
            .map(|line| line.text.clone())
            .unwrap_or_else(|| {
                if is_error {
                    "failed".to_string()
                } else {
                    "completed".to_string()
                }
            });
        let prefix = if index == 0 { "  ⎿  " } else { "     " };
        let style = match summary_result
            .lines
            .first()
            .map(|line| line.tone)
            .unwrap_or(ToolSummaryTone::Neutral)
        {
            ToolSummaryTone::Neutral => dim,
            ToolSummaryTone::Success => ratatui::style::Style::default().fg(Color::LightGreen),
            ToolSummaryTone::Warning => ratatui::style::Style::default().fg(Color::Yellow),
        };
        result.push((format!("{}{} · {}", prefix, target, detail), style));
    }

    if batch.items.len() > max_items {
        result.push((
            format!(
                "     … +{} more exploration steps (ctrl+o to expand)",
                batch.items.len() - max_items
            ),
            dim,
        ));
    }
}

pub(super) fn render_standalone_result(
    entry: &ChatEntry,
    result: &mut Vec<(String, ratatui::style::Style)>,
    dim: ratatui::style::Style,
    accent: ratatui::style::Style,
    red: ratatui::style::Style,
) {
    let ChatRole::ToolResult { name, is_error, .. } = &entry.role else {
        return;
    };

    let summary_result = summarize_tool_result(
        name,
        &serde_json::Value::Null,
        entry.tool_metadata.as_ref(),
        &entry.content,
        *is_error,
    );
    result.push((
        format!("⏺ {}", tool_display_name(name)),
        if *is_error { red } else { accent },
    ));
    if let Some(metadata) = entry.tool_metadata.as_ref() {
        render_metadata_hints(result, metadata, dim);
    }
    let green = ratatui::style::Style::default().fg(Color::LightGreen);
    let red_dim = ratatui::style::Style::default().fg(Color::LightRed);
    render_summary_lines(result, &summary_result.lines, dim, green, red, red_dim);
    if !summary_result.hide_body_by_default {
        if matches!(name.as_str(), "bash" | "powershell") {
            render_shell_output_lines(result, &entry.content, dim, red);
        } else {
            let max_lines = 3;
            let max_line_chars = crossterm::terminal::size()
                .map(|(width, _)| (width as usize).saturating_sub(10))
                .unwrap_or(120);
            for (line_index, line) in entry.content.lines().enumerate() {
                if line_index >= max_lines {
                    result.push((
                        format!(
                            "     … +{} lines (ctrl+o to expand)",
                            entry.content.lines().count() - max_lines
                        ),
                        dim,
                    ));
                    break;
                }
                let prefix = if line_index == 0 { "  ⎿  " } else { "     " };
                let style = if *is_error { red } else { dim };
                let display = truncate_line(line, max_line_chars);
                result.push((format!("{}{}", prefix, display), style));
            }
        }
    }
}

fn render_shell_output_lines(
    result: &mut Vec<(String, ratatui::style::Style)>,
    content: &str,
    stdout_style: ratatui::style::Style,
    stderr_style: ratatui::style::Style,
) {
    let sections = parse_shell_output_sections(content);
    let max_line_chars = crossterm::terminal::size()
        .map(|(width, _)| (width as usize).saturating_sub(10))
        .unwrap_or(120);

    if !sections.stdout_lines.is_empty() && sections.stderr_lines.is_empty() && sections.exit_code.is_none() {
        render_output_lines(result, &sections.stdout_lines, "  ⎿  ", "     ", stdout_style, max_line_chars, 6);
        return;
    }

    if !sections.stdout_lines.is_empty() {
        result.push(("  │ stdout".to_string(), stdout_style));
        render_output_lines(result, &sections.stdout_lines, "     ", "     ", stdout_style, max_line_chars, 5);
    }
    if !sections.stderr_lines.is_empty() {
        result.push(("  │ stderr".to_string(), stderr_style));
        render_output_lines(result, &sections.stderr_lines, "     ", "     ", stderr_style, max_line_chars, 5);
    }
    if let Some(exit_code) = sections.exit_code {
        result.push((format!("  │ exit code {}", exit_code), ratatui::style::Style::default().fg(Color::Yellow)));
    }
}

fn render_output_lines(
    result: &mut Vec<(String, ratatui::style::Style)>,
    lines: &[String],
    first_prefix: &str,
    rest_prefix: &str,
    style: ratatui::style::Style,
    max_line_chars: usize,
    max_lines: usize,
) {
    for (index, line) in lines.iter().enumerate() {
        if index >= max_lines {
            result.push((
                format!("     … +{} lines (ctrl+o to expand)", lines.len() - max_lines),
                style,
            ));
            break;
        }
        let prefix = if index == 0 { first_prefix } else { rest_prefix };
        result.push((format!("{}{}", prefix, truncate_line(line, max_line_chars)), style));
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
                format!(
                    "     … +{} lines (ctrl+o to expand)",
                    total_lines - max_preview
                ),
                dim,
            ));
            break;
        }
        result.push((format!("     + {}", line), green));
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

fn tool_display_name(name: &str) -> String {
    match name {
        "bash" => "Bash".to_string(),
        "powershell" => "PowerShell".to_string(),
        "lsp" => "LSP".to_string(),
        "read_file" => "Read".to_string(),
        "write_file" => "Write".to_string(),
        "edit_file" => "Edit".to_string(),
        "project_map" => "Project Map".to_string(),
        "web_search" => "Web Search".to_string(),
        "web_fetch" => "Web Fetch".to_string(),
        "discover_skills" => "Discover Skills".to_string(),
        other => other
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
            .join(" "),
    }
}

fn group_item_target(name: &str, args: &serde_json::Value) -> String {
    match name {
        "read_file" => display_file_path(args["file_path"].as_str().unwrap_or("???")).to_string(),
        "grep" | "glob" => truncate_line(args["pattern"].as_str().unwrap_or("???"), 40),
        "ls" => display_file_path(args["path"].as_str().unwrap_or(".")).to_string(),
        "web_search" => truncate_line(args["query"].as_str().unwrap_or("web"), 48),
        "web_fetch" => truncate_line(args["url"].as_str().unwrap_or("page"), 48),
        "project_map" => "workspace".to_string(),
        "memory" => truncate_line(args["name"].as_str().unwrap_or("memories"), 40),
        "skill" => truncate_line(args["name"].as_str().unwrap_or("skills"), 40),
        "discover_skills" => "available skills".to_string(),
        "lsp" => {
            let operation = args["operation"].as_str().unwrap_or("lsp");
            let file = args["filePath"].as_str().unwrap_or("");
            if file.is_empty() {
                operation.to_string()
            } else {
                format!("{} {}", operation, display_file_path(file))
            }
        }
        _ => tool_summary_str(name, args),
    }
}

fn render_summary_lines(
    result: &mut Vec<(String, ratatui::style::Style)>,
    lines: &[crate::tool_output_summary::ToolSummaryLine],
    dim: ratatui::style::Style,
    green: ratatui::style::Style,
    _red: ratatui::style::Style,
    _red_dim: ratatui::style::Style,
) {
    for (index, line) in lines.iter().enumerate() {
        let prefix = if index == 0 { "  ⎿  " } else { "     " };
        let style = match line.tone {
            ToolSummaryTone::Neutral => dim,
            ToolSummaryTone::Success => green,
            ToolSummaryTone::Warning => ratatui::style::Style::default().fg(Color::Yellow),
        };
        result.push((format!("{}{}", prefix, line.text), style));
    }
}

#[cfg(test)]
mod tests {
    use ratatui::style::Style;

    use crate::app::{ChatEntry, ChatRole};
    use crate::tool_grouping::detect_groupable_tool_batch;

    use super::{render_grouped_tool_call, render_standalone_result, render_tool_call};

    #[test]
    fn scrollback_grouped_tool_call_uses_exploration_summary_title() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "glob".to_string(),
                },
                "{\"pattern\":\"src/**/*.rs\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "a".to_string(),
                    name: "glob".to_string(),
                    is_error: false,
                },
                "src/main.rs".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "b".to_string(),
                    name: "read_file".to_string(),
                },
                "{\"file_path\":\"/tmp/src/main.rs\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "b".to_string(),
                    name: "read_file".to_string(),
                    is_error: false,
                },
                "fn main() {}".to_string(),
            ),
        ];

        let batch = detect_groupable_tool_batch(&entries, 0).unwrap();
        let mut result = Vec::new();
        render_grouped_tool_call(
            &entries,
            &batch,
            &mut result,
            Style::default(),
            Style::default(),
        );

        assert!(result[0].0.contains("Searched for 1 pattern, read 1 file"));
    }

    #[test]
    fn scrollback_grouped_tool_call_renders_web_and_lsp_targets() {
        let mut entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "web_search".to_string(),
                },
                "{\"query\":\"ratatui status summary\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "a".to_string(),
                    name: "web_search".to_string(),
                    is_error: false,
                },
                "1. Result".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "b".to_string(),
                    name: "lsp".to_string(),
                },
                "{\"operation\":\"hover\",\"filePath\":\"/tmp/src/main.rs\",\"line\":1,\"character\":1}"
                    .to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "b".to_string(),
                    name: "lsp".to_string(),
                    is_error: false,
                },
                "{\"contents\":\"demo\"}".to_string(),
            ),
        ];
        entries[1].tool_metadata = Some(serde_json::json!({
            "query": "ratatui status summary",
            "result_count": 1
        }));
        entries[3].tool_metadata = Some(serde_json::json!({
            "operation": "hover",
            "file_path": "/tmp/src/main.rs",
            "line": 1,
            "character": 1
        }));

        let batch = detect_groupable_tool_batch(&entries, 0).unwrap();
        let mut result = Vec::new();
        render_grouped_tool_call(
            &entries,
            &batch,
            &mut result,
            Style::default(),
            Style::default(),
        );

        assert!(result[0]
            .0
            .contains("Searched the web for 1 query, inspected 1 symbol"));
        assert!(result[1].0.contains("ratatui status summary"));
        assert!(result[2].0.contains("hover /tmp/src/main.rs"));
    }

    #[test]
    fn scrollback_standalone_tool_call_uses_human_friendly_display_name() {
        let call = ChatEntry::new(
            ChatRole::ToolCall {
                id: "a".to_string(),
                name: "read_file".to_string(),
            },
            "{\"file_path\":\"/tmp/src/main.rs\"}".to_string(),
        );
        let result_entry = ChatEntry::new(
            ChatRole::ToolResult {
                id: "a".to_string(),
                name: "read_file".to_string(),
                is_error: false,
            },
            "fn main() {}".to_string(),
        );
        let entries = vec![call.clone(), result_entry];
        let mut result = Vec::new();
        render_tool_call(
            &call,
            &entries,
            0,
            "a",
            "read_file",
            &mut result,
            Style::default(),
            Style::default(),
            Style::default(),
        );
        assert!(result[0].0.contains("⏺ Read .../src/main.rs"));
        assert!(!result[0].0.contains("Read_file"));
    }

    #[test]
    fn scrollback_tool_call_renders_metadata_hints() {
        let call = ChatEntry::new(
            ChatRole::ToolCall {
                id: "a".to_string(),
                name: "powershell".to_string(),
            },
            "{\"command\":\"Get-Content foo.txt\"}".to_string(),
        );
        let mut result_entry = ChatEntry::new(
            ChatRole::ToolResult {
                id: "a".to_string(),
                name: "powershell".to_string(),
                is_error: false,
            },
            "ok".to_string(),
        );
        result_entry.tool_metadata = Some(serde_json::json!({
            "read_only_reason": "validated git status",
            "destructive_warning": "may discard changes",
            "rewrite_suggestion": "Prefer read_file"
        }));
        let entries = vec![call.clone(), result_entry];
        let mut result = Vec::new();
        render_tool_call(
            &call,
            &entries,
            0,
            "a",
            "powershell",
            &mut result,
            Style::default(),
            Style::default(),
            Style::default(),
        );
        assert!(result.iter().any(|line| line.0.contains("read-only: validated git status")));
        assert!(result.iter().any(|line| line.0.contains("warning: may discard changes")));
        assert!(result.iter().any(|line| line.0.contains("hint: Prefer read_file")));
    }

    #[test]
    fn scrollback_standalone_result_uses_summary_and_metadata() {
        let mut entry = ChatEntry::new(
            ChatRole::ToolResult {
                id: "a".to_string(),
                name: "powershell".to_string(),
                is_error: false,
            },
            "ok".to_string(),
        );
        entry.tool_metadata = Some(serde_json::json!({
            "read_only_reason": "validated git status",
            "command_type": "read",
            "rewrite_suggestion": "Prefer read_file"
        }));
        let mut result = Vec::new();
        render_standalone_result(
            &entry,
            &mut result,
            Style::default(),
            Style::default(),
            Style::default(),
        );
        assert!(result[0].0.contains("PowerShell"));
        assert!(result.iter().any(|line| line.0.contains("read-only: validated git status")));
        assert!(result.iter().any(|line| line.0.contains("Prefer read_file")));
    }

    #[test]
    fn scrollback_shell_output_splits_stdout_stderr_and_exit_code() {
        let entry = ChatEntry::new(
            ChatRole::ToolResult {
                id: "a".to_string(),
                name: "bash".to_string(),
                is_error: false,
            },
            "ok\n[stderr]\nwarn\n[exit code: 2]".to_string(),
        );
        let mut result = Vec::new();
        render_standalone_result(
            &entry,
            &mut result,
            Style::default(),
            Style::default(),
            Style::default(),
        );
        assert!(result.iter().any(|line| line.0.contains("stdout")));
        assert!(result.iter().any(|line| line.0.contains("stderr")));
        assert!(result.iter().any(|line| line.0.contains("exit code 2")));
    }
}
