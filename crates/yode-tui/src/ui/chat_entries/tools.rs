use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::folding::{
    render_bash_preview_lines, render_edit_preview_lines, render_folded_result_lines,
    render_write_preview_lines,
};
use super::metadata::render_metadata_lines;
use super::tool_helpers::{tool_summary_value, truncate_ellipsis};
use crate::app::{ChatEntry, ChatRole};
use crate::tool_grouping::ToolBatch;
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
    let tool_display = capitalize_tool(name);

    let mut title_spans = vec![
        Span::styled("⏺ ", Style::default().fg(ACCENT)),
        Span::styled(
            format!("{}(", tool_display),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::styled(truncate_ellipsis(&summary, 60), Style::default().fg(DIM)),
        Span::styled(")", Style::default().fg(WHITE).add_modifier(Modifier::BOLD)),
    ];

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

    render_tool_summary_lines(lines, &summary_result.lines);

    if !summary_result.hide_body_by_default && !result_content.is_empty() {
        render_folded_result_lines(
            lines,
            result_content,
            Style::default().fg(if is_error { RED } else { DIM }),
        );
    }
}

pub(crate) fn render_grouped_tool_call(
    lines: &mut Vec<Line<'static>>,
    all_entries: &[ChatEntry],
    batch: &ToolBatch,
) {
    let title = grouped_tool_display_name(&batch.tool_name);
    lines.push(Line::from(vec![
        Span::styled("⏺ ", Style::default().fg(ACCENT)),
        Span::styled(
            format!("{}({})", title, batch.items.len()),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
    ]));

    let max_items = 4;
    for (index, item) in batch.items.iter().take(max_items).enumerate() {
        let call = &all_entries[item.call_index];
        let args: serde_json::Value = serde_json::from_str(&call.content).unwrap_or_default();
        let result = item
            .result_index
            .and_then(|result_index| all_entries.get(result_index));
        let is_error = result
            .map(|entry| matches!(entry.role, ChatRole::ToolResult { is_error, .. } if is_error))
            .unwrap_or(false);
        let result_content = result.map(|entry| entry.content.as_str()).unwrap_or("");
        let summary_result = summarize_tool_result(
            &batch.tool_name,
            &args,
            result.and_then(|entry| entry.tool_metadata.as_ref()),
            result_content,
            is_error,
        );
        let target = truncate_ellipsis(&group_item_target(&batch.tool_name, &args), 48);
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
        let tone = summary_result
            .lines
            .first()
            .map(|line| line.tone)
            .unwrap_or(if is_error {
                ToolSummaryTone::Warning
            } else {
                ToolSummaryTone::Neutral
            });
        let style = match tone {
            ToolSummaryTone::Neutral => Style::default().fg(DIM),
            ToolSummaryTone::Success => Style::default().fg(GREEN),
            ToolSummaryTone::Warning => Style::default().fg(WARNING_COLOR),
        };
        lines.push(Line::from(Span::styled(
            format!("{}{} · {}", prefix, target, detail),
            style,
        )));
    }

    if batch.items.len() > max_items {
        lines.push(Line::from(Span::styled(
            format!(
                "     … +{} more {} calls (ctrl+o to expand)",
                batch.items.len() - max_items,
                grouped_tool_display_name(&batch.tool_name).to_ascii_lowercase()
            ),
            Style::default().fg(DIM),
        )));
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
            Span::styled("  ⎿ ", Style::default().fg(ACCENT)),
            Span::styled(
                name.clone(),
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            ),
        ]));
        render_tool_summary_lines(lines, &summary_result.lines);
        if !summary_result.hide_body_by_default {
            render_folded_result_lines(
                lines,
                &entry.content,
                Style::default().fg(if *is_error { RED } else { DIM }),
            );
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

fn capitalize_tool(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().to_string() + chars.as_str(),
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

fn grouped_tool_display_name(name: &str) -> &'static str {
    match name {
        "read_file" => "Read",
        "grep" | "glob" => "Search",
        _ => "Tool",
    }
}

fn group_item_target(name: &str, args: &serde_json::Value) -> String {
    match name {
        "read_file" => shorten_path(args["file_path"].as_str().unwrap_or("???")),
        "grep" => truncate_ellipsis(args["pattern"].as_str().unwrap_or("???"), 40),
        "glob" => truncate_ellipsis(args["pattern"].as_str().unwrap_or("???"), 40),
        _ => tool_summary(name, args),
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
