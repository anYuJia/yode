use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::folding::{
    render_bash_preview_lines, render_edit_preview_lines, render_folded_result_lines,
    render_shell_result_lines, render_write_preview_lines,
};
use super::metadata::render_metadata_lines;
use super::tool_helpers::{tool_summary_value, truncate_ellipsis};
use crate::app::{ChatEntry, ChatRole};
use crate::tool_grouping::{
    describe_tool_call, tool_batch_hint_text, tool_batch_summary_text, ToolBatch,
};
use crate::tool_output_summary::{summarize_tool_result, ToolSummaryLine, ToolSummaryTone};
use crate::ui::chat::{ACCENT, DIM, GREEN, RED, WHITE};
use crate::ui::palette::{INFO_COLOR, WARNING_COLOR};

pub(crate) fn render_tool_call(
    lines: &mut Vec<Line<'static>>,
    name: &str,
    args_json: &str,
    result: Option<&ChatEntry>,
    progress: Option<&yode_tools::tool::ToolProgress>,
    timestamp: std::time::Instant,
) {
    let args: serde_json::Value = serde_json::from_str(args_json).unwrap_or_default();
    let is_error = result.map_or(
        false,
        |r| matches!(r.role, ChatRole::ToolResult { is_error, .. } if is_error),
    );
    let result_content = result.map(|r| r.content.as_str()).unwrap_or("");
    let duration = result.and_then(|r| r.duration);
    let summary_result = summarize_tool_result(
        name,
        &args,
        result.and_then(|entry| entry.tool_metadata.as_ref()),
        result_content,
        is_error,
    );

    let summary = tool_summary(name, &args);
    let activity_title = describe_tool_call(name, &args, result.is_none());

    let title_color = if is_error { RED } else { ACCENT };
    let mut title_spans = vec![Span::styled("⏺ ", Style::default().fg(title_color))];
    if let Some(activity_title) = activity_title {
        title_spans.push(Span::styled(
            truncate_ellipsis(&activity_title, 72),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    } else {
        let tool_display = tool_display_name(name);
        title_spans.push(Span::styled(
            format!("{}(", tool_display),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
        title_spans.push(Span::styled(
            truncate_ellipsis(&summary, 60),
            Style::default().fg(DIM),
        ));
        title_spans.push(Span::styled(
            ")",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    }

    if let Some(duration) = duration {
        title_spans.push(Span::styled(
            format!(" [{:.1}s]", duration.as_secs_f32()),
            Style::default().fg(DIM),
        ));
    } else if result.is_none() {
        let elapsed = timestamp.elapsed();
        title_spans.push(Span::styled(
            format!(" [{:.1}s]", elapsed.as_secs_f32()),
            Style::default()
                .fg(WARNING_COLOR)
                .add_modifier(Modifier::ITALIC),
        ));
    }
    if is_error {
        if let Some(error_type) = result.and_then(|entry| entry.tool_error_type.as_deref()) {
            title_spans.push(Span::styled(
                format!(" <{}>", error_type),
                Style::default().fg(RED).add_modifier(Modifier::BOLD),
            ));
        }
    }

    lines.push(Line::from(title_spans));

    if let Some(progress) = progress {
        let mut progress_spans = vec![
            Span::styled("  │ ", Style::default().fg(INFO_COLOR)),
            Span::styled(
                progress.message.clone(),
                Style::default()
                    .fg(INFO_COLOR)
                    .add_modifier(Modifier::ITALIC),
            ),
        ];
        if let Some(percent) = progress.percent {
            progress_spans.push(Span::styled(
                format!(" {}%", percent),
                Style::default().fg(INFO_COLOR).add_modifier(Modifier::BOLD),
            ));
        }
        lines.push(Line::from(progress_spans));
    }

    if let Some(metadata) = result.and_then(|entry| entry.tool_metadata.as_ref()) {
        render_metadata_lines(lines, metadata);
    }

    let has_metadata_diff = result
        .and_then(|entry| entry.tool_metadata.as_ref())
        .and_then(|metadata| metadata.get("diff_preview"))
        .is_some();
    match name {
        "bash" => render_bash_preview_lines(lines, args["command"].as_str().unwrap_or("")),
        "write_file" if !has_metadata_diff => render_write_preview_lines(
            lines,
            args["content"].as_str().unwrap_or(""),
            Style::default().fg(GREEN),
        ),
        "edit_file" if !has_metadata_diff => render_edit_preview_lines(
            lines,
            args["old_string"].as_str().unwrap_or(""),
            args["new_string"].as_str().unwrap_or(""),
            Style::default().fg(RED),
            Style::default().fg(GREEN),
        ),
        _ => {}
    }

    if name == "batch" && !summary_result.lines.is_empty() {
        render_tool_summary_lines(lines, &summary_result.lines[1..]);
    } else {
        render_tool_summary_lines(lines, &summary_result.lines);
    }

    if !summary_result.hide_body_by_default
        && summary_result.lines.is_empty()
        && !result_content.is_empty()
    {
        if matches!(name, "bash" | "powershell") {
            render_shell_result_lines(
                lines,
                result_content,
                Style::default().fg(DIM),
                Style::default().fg(RED),
                Style::default().fg(WARNING_COLOR),
            );
        } else {
            render_folded_result_lines(
                lines,
                result_content,
                Style::default().fg(if is_error { RED } else { DIM }),
            );
        }
    }
}

pub(crate) fn render_grouped_tool_call(
    lines: &mut Vec<Line<'static>>,
    all_entries: &[ChatEntry],
    batch: &ToolBatch,
) {
    lines.push(Line::from(vec![
        Span::styled("⏺ ", Style::default().fg(ACCENT)),
        Span::styled(
            tool_batch_summary_text(batch),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " (ctrl+o to expand)",
            Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
        ),
    ]));
    if let Some(hint) = tool_batch_hint_text(all_entries, batch) {
        lines.push(Line::from(vec![
            Span::styled("  ⎿ ", Style::default().fg(INFO_COLOR)),
            Span::styled(
                truncate_ellipsis(&hint, 84),
                Style::default().fg(INFO_COLOR).add_modifier(Modifier::ITALIC),
            ),
        ]));
    }
}

pub(crate) fn render_standalone_result(lines: &mut Vec<Line<'static>>, entry: &ChatEntry) {
    if let ChatRole::ToolResult { name, is_error, .. } = &entry.role {
        let summary_result = summarize_tool_result(
            name,
            &serde_json::Value::Null,
            entry.tool_metadata.as_ref(),
            &entry.content,
            *is_error,
        );
        lines.push(Line::from(vec![
            Span::styled(
                "  ⎿ ",
                Style::default().fg(if *is_error { RED } else { ACCENT }),
            ),
            Span::styled(
                tool_display_name(name),
                Style::default()
                    .fg(if *is_error { RED } else { WHITE })
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        if let Some(metadata) = entry.tool_metadata.as_ref() {
            render_metadata_lines(lines, metadata);
        }
        render_tool_summary_lines(lines, &summary_result.lines);
        if !summary_result.hide_body_by_default
            && summary_result.lines.is_empty()
            && !entry.content.is_empty()
        {
            if matches!(name.as_str(), "bash" | "powershell") {
                render_shell_result_lines(
                    lines,
                    &entry.content,
                    Style::default().fg(DIM),
                    Style::default().fg(RED),
                    Style::default().fg(WARNING_COLOR),
                );
            } else {
                render_folded_result_lines(
                    lines,
                    &entry.content,
                    Style::default().fg(if *is_error { RED } else { DIM }),
                );
            }
        }
    }
}

fn tool_summary(name: &str, args: &serde_json::Value) -> String {
    match name {
        "edit_file" => {
            let path = tool_summary_value(name, args);
            shorten_path(&path)
        }
        _ => tool_summary_value(name, args),
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

fn shorten_path(path: &str) -> String {
    let parts: Vec<&str> = path.rsplitn(3, '/').collect();
    if parts.len() >= 3 {
        format!(".../{}/{}", parts[1], parts[0])
    } else {
        path.to_string()
    }
}

fn render_tool_summary_lines(lines: &mut Vec<Line<'static>>, summary_lines: &[ToolSummaryLine]) {
    for (index, summary) in summary_lines.iter().enumerate() {
        let prefix = if index == 0 { "  ⎿  " } else { "     " };
        let style = match summary.tone {
            ToolSummaryTone::Neutral => Style::default().fg(DIM),
            ToolSummaryTone::Success => Style::default().fg(GREEN),
            ToolSummaryTone::Warning => Style::default().fg(WARNING_COLOR),
        };
        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, summary.text),
            style,
        )));
    }
}

#[cfg(test)]
mod tests {
    use ratatui::text::Line;

    use crate::app::{ChatEntry, ChatRole};
    use crate::tool_grouping::detect_groupable_tool_batch;

    use super::{render_grouped_tool_call, render_standalone_result, render_tool_call};

    #[test]
    fn grouped_tool_call_uses_exploration_summary_title() {
        let mut entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "grep".to_string(),
                },
                "{\"pattern\":\"retry\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "a".to_string(),
                    name: "grep".to_string(),
                    is_error: false,
                },
                "src/app.rs:12: retry".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "b".to_string(),
                    name: "read_file".to_string(),
                },
                "{\"file_path\":\"/tmp/src/app.rs\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "b".to_string(),
                    name: "read_file".to_string(),
                    is_error: false,
                },
                "fn retry() {}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "c".to_string(),
                    name: "ls".to_string(),
                },
                "{\"path\":\"/tmp/src\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "c".to_string(),
                    name: "ls".to_string(),
                    is_error: false,
                },
                "app.rs".to_string(),
            ),
        ];
        entries[1].tool_metadata = Some(serde_json::json!({
            "output_mode": "content",
            "line_count": 1,
            "file_count": 1,
            "match_count": 1,
            "pattern": "retry"
        }));
        entries[3].tool_metadata = Some(serde_json::json!({
            "file_path": "/tmp/src/app.rs",
            "total_lines": 40,
            "start_line": 1,
            "end_line": 20,
            "was_truncated": true
        }));
        entries[5].tool_metadata = Some(serde_json::json!({
            "path": "/tmp/src",
            "file_count": 1,
            "dir_count": 0,
            "recursive": false
        }));

        let batch = detect_groupable_tool_batch(&entries, 0).unwrap();
        let mut lines = Vec::new();
        render_grouped_tool_call(&mut lines, &entries, &batch);

        assert!(lines[0]
            .to_string()
            .contains("Searched for 1 pattern, read 1 file, listed 1 directory"));
        assert!(lines[0].to_string().contains("ctrl+o to expand"));
        assert!(lines[1].to_string().contains("Listed"));
    }

    #[test]
    fn grouped_tool_call_renders_web_and_lsp_targets() {
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
        let mut lines = Vec::new();
        render_grouped_tool_call(&mut lines, &entries, &batch);

        assert!(lines[0]
            .to_string()
            .contains("Searched the web for 1 query, inspected 1 symbol"));
        assert!(lines[0].to_string().contains("ctrl+o to expand"));
        assert!(lines[1].to_string().contains("Inspected"));
    }

    #[test]
    fn standalone_tool_call_uses_human_friendly_display_name() {
        let entry = ChatEntry::new(
            ChatRole::ToolResult {
                id: "a".to_string(),
                name: "read_file".to_string(),
                is_error: false,
            },
            "fn main() {}".to_string(),
        );
        let call = ChatEntry::new(
            ChatRole::ToolCall {
                id: "a".to_string(),
                name: "read_file".to_string(),
            },
            "{\"file_path\":\"/tmp/src/main.rs\"}".to_string(),
        );
        let mut lines = Vec::new();
        render_tool_call(
            &mut lines,
            "read_file",
            &call.content,
            Some(&entry),
            None,
            call.timestamp,
        );
        assert!(lines[0].to_string().contains("Read .../src/main.rs"));
        assert!(!lines[0].to_string().contains("Read_file"));
    }

    #[test]
    fn standalone_result_renders_metadata_hints() {
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
            "rewrite_suggestion": "Prefer read_file"
        }));
        let mut lines: Vec<Line<'static>> = Vec::new();
        render_standalone_result(&mut lines, &entry);
        assert!(lines
            .iter()
            .any(|line| line.to_string().contains("read-only: validated git status")));
        assert!(lines
            .iter()
            .any(|line| line.to_string().contains("hint: Prefer read_file")));
    }

    #[test]
    fn standalone_shell_result_hides_verbose_sections() {
        let entry = ChatEntry::new(
            ChatRole::ToolResult {
                id: "a".to_string(),
                name: "bash".to_string(),
                is_error: false,
            },
            "ok\n[stderr]\nwarn\n[exit code: 2]".to_string(),
        );
        let mut lines: Vec<Line<'static>> = Vec::new();
        render_standalone_result(&mut lines, &entry);
        assert!(lines[0].to_string().contains("Bash"));
        assert!(lines
            .iter()
            .all(|line| !line.to_string().contains("stdout")));
        assert!(lines
            .iter()
            .all(|line| !line.to_string().contains("stderr")));
        assert!(lines
            .iter()
            .all(|line| !line.to_string().contains("exit code 2")));
    }
}
