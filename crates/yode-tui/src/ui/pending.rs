use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;

pub fn queued_input_preview(display: &str, max_chars: usize) -> String {
    let first_line = display.lines().next().unwrap_or("");
    if first_line.chars().count() <= max_chars {
        first_line.to_string()
    } else {
        format!("{}...", first_line.chars().take(max_chars).collect::<String>())
    }
}

pub fn render_pending_inputs(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    if area.height == 0 || app.pending_inputs.is_empty() {
        return;
    }

    let mut lines = Vec::new();
    for (display, _) in &app.pending_inputs {
        lines.push(Line::from(vec![
            Span::styled("  > ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                queued_input_preview(display, 80),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(" (queued)", Style::default().fg(Color::DarkGray)),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), area);
}
