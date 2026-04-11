use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::{ChatEntry, ChatRole};
use crate::ui::chat::{ACCENT, DIM, GREEN, RED, WHITE, YELLOW};

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

    let summary = tool_summary(name, &args);
    let tool_display = capitalize_tool(name);

    let mut title_spans = vec![
        Span::styled("⏺ ", Style::default().fg(ACCENT)),
        Span::styled(
            format!("{}(", tool_display),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::styled(truncate_str(&summary, 60), Style::default().fg(DIM)),
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
            Style::default().fg(YELLOW).add_modifier(Modifier::ITALIC),
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
            Span::styled("  │ ", Style::default().fg(YELLOW)),
            Span::styled(
                progress.message.clone(),
                Style::default().fg(YELLOW).add_modifier(Modifier::ITALIC),
            ),
        ];
        if let Some(percent) = progress.percent {
            progress_spans.push(Span::styled(
                format!(" {}%", percent),
                Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
            ));
        }
        lines.push(Line::from(progress_spans));
    }

    if let Some(metadata) = result.and_then(|entry| entry.tool_metadata.as_ref()) {
        render_metadata(lines, metadata);
    }

    let has_metadata_diff = result
        .and_then(|entry| entry.tool_metadata.as_ref())
        .and_then(|metadata| metadata.get("diff_preview"))
        .is_some();
    match name {
        "bash" => render_bash_content(lines, &args),
        "write_file" if !has_metadata_diff => render_write_content(lines, &args),
        "edit_file" if !has_metadata_diff => render_edit_content(lines, &args),
        _ => {}
    }

    if !result_content.is_empty() {
        render_result_content(lines, result_content, is_error);
    }
}

pub(crate) fn render_standalone_result(lines: &mut Vec<Line<'static>>, entry: &ChatEntry) {
    if let ChatRole::ToolResult { name, is_error, .. } = &entry.role {
        let color = if *is_error { RED } else { DIM };
        lines.push(Line::from(vec![
            Span::styled("  ⎿ ", Style::default().fg(ACCENT)),
            Span::styled(
                name.clone(),
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            ),
        ]));
        for (index, line) in entry.content.lines().enumerate() {
            if index >= 5 {
                lines.push(Line::from(Span::styled(
                    format!("     … {} more lines", entry.content.lines().count() - 5),
                    Style::default().fg(DIM),
                )));
                break;
            }
            lines.push(Line::from(Span::styled(
                format!("     {}", line),
                Style::default().fg(color),
            )));
        }
    }
}

fn render_metadata(lines: &mut Vec<Line<'static>>, metadata: &serde_json::Value) {
    if let Some(diff) = metadata.get("diff_preview").and_then(|value| value.as_object()) {
        let removed = diff
            .get("removed")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str())
            .take(5)
            .collect::<Vec<_>>();
        let added = diff
            .get("added")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str())
            .take(5)
            .collect::<Vec<_>>();

        for line in removed {
            lines.push(Line::from(Span::styled(
                format!("     - {}", line),
                Style::default().fg(RED),
            )));
        }
        for line in added {
            lines.push(Line::from(Span::styled(
                format!("     + {}", line),
                Style::default().fg(GREEN),
            )));
        }
    }
    if let Some(truncation) = metadata
        .get("tool_runtime")
        .and_then(|value| value.get("truncation"))
        .and_then(|value| value.as_object())
    {
        if let Some(reason) = truncation.get("reason").and_then(|value| value.as_str()) {
            lines.push(Line::from(Span::styled(
                format!("  │ truncated: {}", reason),
                Style::default().fg(YELLOW),
            )));
        }
    }
}

fn render_result_content(lines: &mut Vec<Line<'static>>, result_content: &str, is_error: bool) {
    let output_lines: Vec<&str> = result_content.lines().collect();
    let max_show = 8;
    let show = output_lines.len().min(max_show);
    let result_color = if is_error { RED } else { DIM };

    for (index, line) in output_lines[..show].iter().enumerate() {
        let prefix = if index == 0 { "  ⎿  " } else { "     " };
        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, line),
            Style::default().fg(result_color),
        )));
    }
    if output_lines.len() > max_show {
        lines.push(Line::from(Span::styled(
            format!("     … {} more lines", output_lines.len() - max_show),
            Style::default().fg(DIM),
        )));
    }
}

fn tool_summary(name: &str, args: &serde_json::Value) -> String {
    match name {
        "bash" => args["command"].as_str().unwrap_or("???").to_string(),
        "write_file" | "read_file" => args["file_path"].as_str().unwrap_or("???").to_string(),
        "edit_file" => {
            let path = args["file_path"].as_str().unwrap_or("???");
            shorten_path(path)
        }
        "glob" => args["pattern"].as_str().unwrap_or("???").to_string(),
        "grep" => args["pattern"].as_str().unwrap_or("???").to_string(),
        _ => {
            if let Some(obj) = args.as_object() {
                for key in &["command", "path", "file_path", "query", "pattern", "url"] {
                    if let Some(val) = obj.get(*key) {
                        if let Some(text) = val.as_str() {
                            return text.to_string();
                        }
                    }
                }
            }
            String::new()
        }
    }
}

fn capitalize_tool(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().to_string() + chars.as_str(),
    }
}

fn render_bash_content(lines: &mut Vec<Line<'static>>, args: &serde_json::Value) {
    let command = args["command"].as_str().unwrap_or("");
    if command.contains('\n') {
        for line in command.lines().take(4) {
            lines.push(Line::from(Span::styled(
                format!("     {}", line),
                Style::default().fg(Color::Gray),
            )));
        }
    }
}

fn render_write_content(lines: &mut Vec<Line<'static>>, args: &serde_json::Value) {
    let content = args["content"].as_str().unwrap_or("");
    let line_count = content.lines().count();
    if line_count > 0 {
        for line in content.lines().take(5) {
            lines.push(Line::from(Span::styled(
                format!("     + {}", line),
                Style::default().fg(GREEN),
            )));
        }
        if line_count > 5 {
            lines.push(Line::from(Span::styled(
                format!("     … {} more lines", line_count - 5),
                Style::default().fg(DIM),
            )));
        }
    }
}

fn render_edit_content(lines: &mut Vec<Line<'static>>, args: &serde_json::Value) {
    let old = args["old_string"].as_str().unwrap_or("");
    let new = args["new_string"].as_str().unwrap_or("");
    let max_diff = 5;

    for (index, line) in old.lines().enumerate() {
        if index >= max_diff {
            lines.push(Line::from(Span::styled(
                format!("     … {} more removed", old.lines().count() - max_diff),
                Style::default().fg(RED),
            )));
            break;
        }
        lines.push(Line::from(Span::styled(
            format!("     - {}", line),
            Style::default().fg(RED),
        )));
    }
    for (index, line) in new.lines().enumerate() {
        if index >= max_diff {
            lines.push(Line::from(Span::styled(
                format!("     … {} more added", new.lines().count() - max_diff),
                Style::default().fg(GREEN),
            )));
            break;
        }
        lines.push(Line::from(Span::styled(
            format!("     + {}", line),
            Style::default().fg(GREEN),
        )));
    }
}

fn truncate_str(text: &str, max: usize) -> String {
    if text.len() > max {
        format!("{}...", &text[..max])
    } else {
        text.to_string()
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
