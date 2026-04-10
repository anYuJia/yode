use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::{ChatEntry, ChatRole};
use crate::ui::chat::{
    render_markdown_white, ACCENT, CYAN, DIM, GREEN, RED, WHITE, YELLOW,
};

// Claude Code style: just bold white text, no heavy decoration
pub(super) fn render_user(lines: &mut Vec<Line<'static>>, entry: &ChatEntry) {
    let user_style = Style::default().fg(CYAN);
    for (i, line) in entry.content.lines().enumerate() {
        if i == 0 {
            lines.push(Line::from(vec![
                Span::styled(
                    "> ",
                    Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    line.to_string(),
                    user_style.add_modifier(Modifier::BOLD),
                ),
            ]));
        } else {
            lines.push(Line::from(Span::styled(
                format!("  {}", line),
                user_style,
            )));
        }
    }
}

// Claude Code style: ⏺ prefix on first line, indented continuation
pub(super) fn render_assistant(lines: &mut Vec<Line<'static>>, entry: &ChatEntry) {
    if let Some(ref reasoning) = entry.reasoning {
        if !reasoning.trim().is_empty() {
            lines.push(Line::from(vec![Span::styled(
                "  💭 Thinking…",
                Style::default().fg(YELLOW).add_modifier(Modifier::ITALIC),
            )]));

            for line in reasoning.trim().lines() {
                lines.push(Line::from(vec![
                    Span::styled(
                        "  │ ",
                        Style::default().fg(YELLOW).add_modifier(Modifier::DIM),
                    ),
                    Span::styled(
                        line.to_string(),
                        Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
                    ),
                ]));
            }
            lines.push(Line::from(""));
        }
    }

    let md = render_markdown_white(&entry.content);
    for (i, line) in md.into_iter().enumerate() {
        let mut spans = Vec::new();
        if i == 0 {
            spans.push(Span::styled("⏺ ", Style::default().fg(ACCENT)));
        } else {
            spans.push(Span::raw("  "));
        }
        spans.extend(line.spans);
        lines.push(Line::from(spans));
    }
}

// Claude Code style: ⏺ ToolName(summary) [duration] then ⎿ result
pub(super) fn render_tool_call(
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

    if let Some(d) = duration {
        title_spans.push(Span::styled(
            format!(" [{:.1}s]", d.as_secs_f32()),
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

    if let Some(p) = progress {
        let mut progress_spans = vec![
            Span::styled("  │ ", Style::default().fg(YELLOW)),
            Span::styled(
                p.message.clone(),
                Style::default().fg(YELLOW).add_modifier(Modifier::ITALIC),
            ),
        ];
        if let Some(pct) = p.percent {
            progress_spans.push(Span::styled(
                format!(" {}%", pct),
                Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
            ));
        }
        lines.push(Line::from(progress_spans));
    }

    if let Some(metadata) = result.and_then(|entry| entry.tool_metadata.as_ref()) {
        if let Some(diff) = metadata
            .get("diff_preview")
            .and_then(|value| value.as_object())
        {
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
        let output_lines: Vec<&str> = result_content.lines().collect();
        let max_show = 8;
        let show = output_lines.len().min(max_show);

        let result_color = if is_error { RED } else { DIM };

        for (i, line) in output_lines[..show].iter().enumerate() {
            let prefix = if i == 0 { "  ⎿  " } else { "     " };
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
}

pub(super) fn render_standalone_result(lines: &mut Vec<Line<'static>>, entry: &ChatEntry) {
    if let ChatRole::ToolResult { name, is_error, .. } = &entry.role {
        let color = if *is_error { RED } else { DIM };
        lines.push(Line::from(vec![
            Span::styled("  ⎿ ", Style::default().fg(ACCENT)),
            Span::styled(
                name.clone(),
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            ),
        ]));
        for (i, line) in entry.content.lines().enumerate() {
            if i >= 5 {
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
                        if let Some(s) = val.as_str() {
                            return s.to_string();
                        }
                    }
                }
            }
            String::new()
        }
    }
}

fn capitalize_tool(name: &str) -> String {
    let mut c = name.chars();
    match c.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().to_string() + c.as_str(),
    }
}

fn render_bash_content(lines: &mut Vec<Line<'static>>, args: &serde_json::Value) {
    let cmd = args["command"].as_str().unwrap_or("");
    if cmd.contains('\n') {
        for line in cmd.lines().take(4) {
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

    for (i, line) in old.lines().enumerate() {
        if i >= max_diff {
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
    for (i, line) in new.lines().enumerate() {
        if i >= max_diff {
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

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max])
    } else {
        s.to_string()
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
