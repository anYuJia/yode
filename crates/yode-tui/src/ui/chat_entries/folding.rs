use ratatui::style::Style;
use ratatui::text::{Line, Span};

pub(super) fn render_folded_result_lines(
    lines: &mut Vec<Line<'static>>,
    result_content: &str,
    result_style: Style,
) {
    let output_lines: Vec<&str> = result_content.lines().collect();
    let max_show = 8;
    let show = output_lines.len().min(max_show);

    for (index, line) in output_lines[..show].iter().enumerate() {
        let prefix = if index == 0 { "  ⎿  " } else { "     " };
        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, line),
            result_style,
        )));
    }
    if output_lines.len() > max_show {
        lines.push(Line::from(Span::styled(
            format!("     … {} more lines", output_lines.len() - max_show),
            Style::default().fg(ratatui::style::Color::Gray),
        )));
    }
}

pub(super) fn render_bash_preview_lines(
    lines: &mut Vec<Line<'static>>,
    command: &str,
) {
    if command.contains('\n') {
        for line in command.lines().take(4) {
            lines.push(Line::from(Span::styled(
                format!("     {}", line),
                Style::default().fg(ratatui::style::Color::Gray),
            )));
        }
    }
}

pub(super) fn render_write_preview_lines(
    lines: &mut Vec<Line<'static>>,
    content: &str,
    add_style: Style,
) {
    let line_count = content.lines().count();
    if line_count > 0 {
        for line in content.lines().take(5) {
            lines.push(Line::from(Span::styled(
                format!("     + {}", line),
                add_style,
            )));
        }
        if line_count > 5 {
            lines.push(Line::from(Span::styled(
                format!("     … {} more lines", line_count - 5),
                Style::default().fg(ratatui::style::Color::Gray),
            )));
        }
    }
}

pub(super) fn render_edit_preview_lines(
    lines: &mut Vec<Line<'static>>,
    old: &str,
    new: &str,
    remove_style: Style,
    add_style: Style,
) {
    let max_diff = 5;
    for (index, line) in old.lines().enumerate() {
        if index >= max_diff {
            lines.push(Line::from(Span::styled(
                format!("     … {} more removed", old.lines().count() - max_diff),
                remove_style,
            )));
            break;
        }
        lines.push(Line::from(Span::styled(
            format!("     - {}", line),
            remove_style,
        )));
    }
    for (index, line) in new.lines().enumerate() {
        if index >= max_diff {
            lines.push(Line::from(Span::styled(
                format!("     … {} more added", new.lines().count() - max_diff),
                add_style,
            )));
            break;
        }
        lines.push(Line::from(Span::styled(
            format!("     + {}", line),
            add_style,
        )));
    }
}

pub(crate) fn fold_subagent_tool_calls(
    tool_names: &[String],
    max_show: usize,
) -> Vec<String> {
    let mut lines = Vec::new();
    for (index, tool_name) in tool_names.iter().enumerate() {
        if index >= max_show {
            lines.push(format!(
                "     … +{} more tool uses (ctrl+o to expand)",
                tool_names.len() - max_show
            ));
            break;
        }
        let prefix = if index == 0 { "  ⎿  " } else { "     " };
        lines.push(format!("{}{}(…)", prefix, capitalize(tool_name)));
    }
    if tool_names.is_empty() {
        lines.push("  ⎿  (no tool calls)".to_string());
    }
    lines
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().to_string() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::{fold_subagent_tool_calls, render_folded_result_lines};

    #[test]
    fn fold_subagent_tool_calls_limits_output() {
        let lines = fold_subagent_tool_calls(
            &[
                "bash".to_string(),
                "read_file".to_string(),
                "grep".to_string(),
                "glob".to_string(),
            ],
            3,
        );
        assert_eq!(lines.len(), 4);
        assert!(lines.last().unwrap().contains("more tool uses"));
    }

    #[test]
    fn folded_result_lines_truncate_long_output() {
        let mut lines = Vec::new();
        render_folded_result_lines(
            &mut lines,
            "a\nb\nc\nd\ne\nf\ng\nh\ni",
            ratatui::style::Style::default(),
        );
        assert!(lines
            .last()
            .unwrap()
            .spans
            .first()
            .unwrap()
            .content
            .contains("more lines"));
    }
}
