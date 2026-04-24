use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::app::rendering::truncate_line;
use crate::tool_output_summary::parse_shell_output_sections;

pub(super) fn render_folded_result_lines(
    lines: &mut Vec<Line<'static>>,
    result_content: &str,
    result_style: Style,
) {
    let preview_lines: Vec<&str> = result_content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if let Some(first_line) = preview_lines.first() {
        lines.push(Line::from(Span::styled(
            format!("  ⎿  {}", truncate_line(first_line, 120)),
            result_style,
        )));
    }
    if preview_lines.len() > 1 {
        lines.push(Line::from(Span::styled(
            format!(
                "     … {} more lines (ctrl+o to expand)",
                preview_lines.len() - 1
            ),
            Style::default().fg(ratatui::style::Color::Gray),
        )));
    }
}

pub(super) fn render_shell_result_lines(
    lines: &mut Vec<Line<'static>>,
    result_content: &str,
    stdout_style: Style,
    stderr_style: Style,
    exit_style: Style,
) {
    let sections = parse_shell_output_sections(result_content);
    let stdout_line = sections
        .stdout_lines
        .iter()
        .find(|line| !line.trim().is_empty());
    let stderr_line = sections
        .stderr_lines
        .iter()
        .find(|line| !line.trim().is_empty());
    let total_lines = sections.stdout_lines.len()
        + sections.stderr_lines.len()
        + usize::from(sections.exit_code.is_some());

    if let Some(line) = stdout_line {
        lines.push(Line::from(Span::styled(
            format!("  ⎿  {}", truncate_line(line, 120)),
            stdout_style,
        )));
    } else if let Some(line) = stderr_line {
        lines.push(Line::from(Span::styled(
            format!("  ⎿  {}", truncate_line(line, 120)),
            stderr_style,
        )));
    } else if let Some(exit_code) = sections.exit_code {
        lines.push(Line::from(Span::styled(
            format!("  ⎿  exit code {}", exit_code),
            exit_style,
        )));
    }

    if total_lines > 1 {
        lines.push(Line::from(Span::styled(
            format!("     … {} more lines (ctrl+o to expand)", total_lines - 1),
            Style::default().fg(ratatui::style::Color::Gray),
        )));
    }
}

pub(super) fn render_bash_preview_lines(_lines: &mut Vec<Line<'static>>, _command: &str) {}

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

#[cfg(test)]
mod tests {
    use super::{render_folded_result_lines, render_shell_result_lines};

    #[test]
    fn folded_result_lines_truncate_long_output() {
        let mut lines = Vec::new();
        render_folded_result_lines(
            &mut lines,
            "a\nb\nc\nd\ne\nf\ng\nh\ni",
            ratatui::style::Style::default(),
        );
        assert_eq!(lines[0].to_string(), "  ⎿  a");
        assert!(lines
            .last()
            .unwrap()
            .spans
            .first()
            .unwrap()
            .content
            .contains("more lines"));
    }

    #[test]
    fn shell_result_lines_use_single_preview_and_hide_sections() {
        let mut lines = Vec::new();
        render_shell_result_lines(
            &mut lines,
            "ok\n[stderr]\nwarn\n[exit code: 2]",
            ratatui::style::Style::default(),
            ratatui::style::Style::default(),
            ratatui::style::Style::default(),
        );
        assert_eq!(lines[0].to_string(), "  ⎿  ok");
        assert!(lines[1].to_string().contains("more lines"));
    }
}
