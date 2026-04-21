use ratatui::style::Style;
use ratatui::text::{Line, Span};
use crate::ui::palette::{ERROR_COLOR, INFO_COLOR, SUCCESS_COLOR, WARNING_COLOR};

pub(super) fn render_metadata_lines(
    lines: &mut Vec<Line<'static>>,
    metadata: &serde_json::Value,
) {
    render_tool_hint_lines(lines, metadata);
    render_diff_preview_lines(lines, metadata);
    if let Some(truncation) = metadata
        .get("tool_runtime")
        .and_then(|value| value.get("truncation"))
        .and_then(|value| value.as_object())
    {
        if let Some(reason) = truncation.get("reason").and_then(|value| value.as_str()) {
            lines.push(Line::from(Span::styled(
                format!("  │ truncated: {}", reason),
                Style::default().fg(WARNING_COLOR),
            )));
        }
    }
}

pub(super) fn render_tool_hint_lines(
    lines: &mut Vec<Line<'static>>,
    metadata: &serde_json::Value,
) {
    if let Some(reason) = metadata
        .get("read_only_reason")
        .and_then(|value| value.as_str())
    {
        lines.push(Line::from(Span::styled(
            format!("  │ read-only: {}", reason),
            Style::default().fg(INFO_COLOR),
        )));
    } else if metadata
        .get("read_only")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        lines.push(Line::from(Span::styled(
            "  │ read-only command",
            Style::default().fg(INFO_COLOR),
        )));
    }

    if let Some(warning) = metadata
        .get("destructive_warning")
        .and_then(|value| value.as_str())
    {
        lines.push(Line::from(Span::styled(
            format!("  │ warning: {}", warning),
            Style::default().fg(WARNING_COLOR),
        )));
    }

    if let Some(suggestion) = metadata
        .get("rewrite_suggestion")
        .and_then(|value| value.as_str())
    {
        lines.push(Line::from(Span::styled(
            format!("  │ hint: {}", suggestion),
            Style::default().fg(INFO_COLOR),
        )));
    }
}

pub(super) fn render_diff_preview_lines(
    lines: &mut Vec<Line<'static>>,
    metadata: &serde_json::Value,
) {
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
                Style::default().fg(ERROR_COLOR),
            )));
        }
        for line in added {
            lines.push(Line::from(Span::styled(
                format!("     + {}", line),
                Style::default().fg(SUCCESS_COLOR),
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::text::Line;

    use super::{render_metadata_lines, render_tool_hint_lines};

    #[test]
    fn metadata_lines_surface_tool_hints() {
        let mut lines: Vec<Line<'static>> = Vec::new();
        render_tool_hint_lines(
            &mut lines,
            &serde_json::json!({
                "read_only_reason": "validated git status",
                "destructive_warning": "may discard changes",
                "rewrite_suggestion": "Prefer read_file"
            }),
        );
        assert!(lines[0].to_string().contains("read-only: validated git status"));
        assert!(lines[1].to_string().contains("warning: may discard changes"));
        assert!(lines[2].to_string().contains("hint: Prefer read_file"));
    }

    #[test]
    fn metadata_lines_keep_diff_preview_rendering() {
        let mut lines: Vec<Line<'static>> = Vec::new();
        render_metadata_lines(
            &mut lines,
            &serde_json::json!({
                "diff_preview": {
                    "removed": ["old"],
                    "added": ["new"]
                }
            }),
        );
        assert!(lines.iter().any(|line| line.to_string().contains("- old")));
        assert!(lines.iter().any(|line| line.to_string().contains("+ new")));
    }
}
