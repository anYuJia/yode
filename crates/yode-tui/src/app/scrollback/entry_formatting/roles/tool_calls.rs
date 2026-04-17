use std::time::Duration;

use ratatui::style::Color;

use crate::app::rendering::{capitalize, truncate_line};
use crate::app::{ChatEntry, ChatRole};
use crate::tool_grouping::ToolBatch;
use crate::tool_output_summary::{summarize_tool_result, ToolSummaryTone};

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
    } else if name == "read_file" {
        let display_path = display_file_path(args["file_path"].as_str().unwrap_or("???"));
        result.push((format!("⏺ Read({}){}", display_path, timing), accent));
    } else if name == "write_file" {
        render_write_file(args, &timing, result, dim, accent, green);
    } else {
        let summary = tool_summary_str(name, &args);
        result.push((
            format!("⏺ {}({}){}", capitalize(name), summary, timing),
            accent,
        ));

        render_summary_lines(result, &summary_result.lines, dim, green, red, red_dim);

        if let Some(res) = tool_result {
            if summary_result.hide_body_by_default {
                return;
            }
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

pub(super) fn render_grouped_tool_call(
    all_entries: &[ChatEntry],
    batch: &ToolBatch,
    result: &mut Vec<(String, ratatui::style::Style)>,
    dim: ratatui::style::Style,
    accent: ratatui::style::Style,
) {
    result.push((
        format!(
            "⏺ {}({})",
            grouped_tool_display_name(&batch.tool_name),
            batch.items.len()
        ),
        accent,
    ));

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
            &batch.tool_name,
            &args,
            tool_result.and_then(|entry| entry.tool_metadata.as_ref()),
            tool_result
                .map(|entry| entry.content.as_str())
                .unwrap_or(""),
            is_error,
        );
        let target = group_item_target(&batch.tool_name, &args);
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
                "     … +{} more {} calls (ctrl+o to expand)",
                batch.items.len() - max_items,
                grouped_tool_display_name(&batch.tool_name).to_ascii_lowercase()
            ),
            dim,
        ));
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

fn grouped_tool_display_name(name: &str) -> &'static str {
    match name {
        "read_file" => "Read",
        "grep" | "glob" => "Search",
        _ => "Tool",
    }
}

fn group_item_target(name: &str, args: &serde_json::Value) -> String {
    match name {
        "read_file" => display_file_path(args["file_path"].as_str().unwrap_or("???")).to_string(),
        "grep" | "glob" => truncate_line(args["pattern"].as_str().unwrap_or("???"), 40),
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
