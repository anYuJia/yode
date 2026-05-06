use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;

pub(crate) const MAX_PENDING_INPUT_LINES: u16 = 3;

pub fn queued_input_preview(display: &str, max_chars: usize) -> String {
    let first_line = display.lines().next().unwrap_or("");
    if first_line.chars().count() <= max_chars {
        first_line.to_string()
    } else {
        format!(
            "{}...",
            first_line.chars().take(max_chars).collect::<String>()
        )
    }
}

pub fn render_pending_inputs(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    if area.height == 0 || app.pending_inputs.is_empty() {
        return;
    }

    let lines = pending_input_lines(&app.pending_inputs, area.height);
    frame.render_widget(Paragraph::new(lines), area);
}

fn pending_input_lines(pending_inputs: &[(String, String)], max_lines: u16) -> Vec<Line<'static>> {
    if max_lines == 0 || pending_inputs.is_empty() {
        return Vec::new();
    }

    let max_lines = max_lines as usize;
    let visible_preview_count = if pending_inputs.len() > max_lines {
        max_lines.saturating_sub(1)
    } else {
        max_lines
    };
    let mut lines = Vec::new();
    for (display, _) in pending_inputs.iter().take(visible_preview_count) {
        lines.push(Line::from(vec![
            Span::styled("  > ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                queued_input_preview(display, 80),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(" (queued)", Style::default().fg(Color::DarkGray)),
        ]));
    }

    if pending_inputs.len() > visible_preview_count {
        lines.push(Line::from(Span::styled(
            format!(
                "  ... {} more queued",
                pending_inputs.len() - visible_preview_count
            ),
            Style::default().fg(Color::DarkGray),
        )));
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::{pending_input_lines, queued_input_preview};

    #[test]
    fn queued_input_preview_uses_first_line_and_truncates() {
        assert_eq!(queued_input_preview("first\nsecond", 20), "first");
        assert_eq!(queued_input_preview("abcdef", 3), "abc...");
    }

    #[test]
    fn pending_input_lines_summarize_overflow() {
        let pending = vec![
            ("one".to_string(), "one".to_string()),
            ("two".to_string(), "two".to_string()),
            ("three".to_string(), "three".to_string()),
            ("four".to_string(), "four".to_string()),
        ];

        let lines = pending_input_lines(&pending, 3);

        assert_eq!(lines.len(), 3);
        assert!(lines[0].to_string().contains("one"));
        assert!(lines[1].to_string().contains("two"));
        assert!(lines[2].to_string().contains("2 more queued"));
    }
}
