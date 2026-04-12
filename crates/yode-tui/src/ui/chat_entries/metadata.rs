use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

pub(super) fn render_metadata_lines(
    lines: &mut Vec<Line<'static>>,
    metadata: &serde_json::Value,
) {
    render_diff_preview_lines(lines, metadata);
    if let Some(truncation) = metadata
        .get("tool_runtime")
        .and_then(|value| value.get("truncation"))
        .and_then(|value| value.as_object())
    {
        if let Some(reason) = truncation.get("reason").and_then(|value| value.as_str()) {
            lines.push(Line::from(Span::styled(
                format!("  │ truncated: {}", reason),
                Style::default().fg(Color::LightYellow),
            )));
        }
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
                Style::default().fg(Color::LightRed),
            )));
        }
        for line in added {
            lines.push(Line::from(Span::styled(
                format!("     + {}", line),
                Style::default().fg(Color::LightGreen),
            )));
        }
    }
}
