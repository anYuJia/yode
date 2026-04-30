use crate::ui::palette::{ERROR_COLOR, INFO_COLOR, SUCCESS_COLOR, WARNING_COLOR};
use ratatui::style::Style;
use ratatui::text::{Line, Span};

pub(super) fn render_metadata_lines(lines: &mut Vec<Line<'static>>, metadata: &serde_json::Value) {
    render_tool_hint_lines(lines, metadata);
    render_diff_preview_lines(lines, metadata);
}

pub(super) fn render_tool_hint_lines(lines: &mut Vec<Line<'static>>, metadata: &serde_json::Value) {
    if let Some(line) = metadata_hint_line(metadata) {
        lines.push(line);
    }
}

fn metadata_hint_line(metadata: &serde_json::Value) -> Option<Line<'static>> {
    let mut spans = vec![Span::styled("  │ ", Style::default().fg(INFO_COLOR))];
    let mut has_segment = false;

    if let Some(reason) = metadata
        .get("read_only_reason")
        .and_then(|value| value.as_str())
    {
        append_hint_segment(
            &mut spans,
            &mut has_segment,
            format!("read-only: {}", reason),
            INFO_COLOR,
        );
    } else if metadata
        .get("read_only")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        append_hint_segment(
            &mut spans,
            &mut has_segment,
            "read-only command".to_string(),
            INFO_COLOR,
        );
    }

    if let Some(warning) = metadata
        .get("destructive_warning")
        .and_then(|value| value.as_str())
    {
        append_hint_segment(
            &mut spans,
            &mut has_segment,
            format!("warning: {}", warning),
            WARNING_COLOR,
        );
    }

    if let Some(suggestion) = metadata
        .get("rewrite_suggestion")
        .and_then(|value| value.as_str())
    {
        append_hint_segment(
            &mut spans,
            &mut has_segment,
            format!("hint: {}", suggestion),
            INFO_COLOR,
        );
    }

    if let Some(reason) = metadata
        .get("tool_runtime")
        .and_then(|value| value.get("truncation"))
        .and_then(|value| value.get("reason"))
        .and_then(|value| value.as_str())
    {
        append_hint_segment(
            &mut spans,
            &mut has_segment,
            format!("truncated: {}", reason),
            WARNING_COLOR,
        );
    }

    has_segment.then_some(Line::from(spans))
}

fn append_hint_segment(
    spans: &mut Vec<Span<'static>>,
    has_segment: &mut bool,
    text: String,
    color: ratatui::style::Color,
) {
    if *has_segment {
        spans.push(Span::styled(" · ", Style::default().fg(INFO_COLOR)));
    }
    spans.push(Span::styled(text, Style::default().fg(color)));
    *has_segment = true;
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
        assert_eq!(lines.len(), 1);
        assert!(lines[0]
            .to_string()
            .contains("read-only: validated git status"));
        assert!(lines[0]
            .to_string()
            .contains("warning: may discard changes"));
        assert!(lines[0].to_string().contains("hint: Prefer read_file"));
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
