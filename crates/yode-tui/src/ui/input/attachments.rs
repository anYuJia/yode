use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;

pub fn render_attachments(frame: &mut Frame, area: Rect, app: &App) {
    let mut spans = Vec::new();
    for attachment in &app.input.attachments {
        spans.push(Span::styled(
            format!(
                "[{} · {}L · {}C · {}] ",
                attachment.name,
                attachment.line_count,
                attachment.char_count,
                attachment_preview(&attachment.content)
            ),
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        ));
    }
    if !spans.is_empty() {
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}

fn attachment_preview(content: &str) -> String {
    let squashed = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if squashed.is_empty() {
        return "empty".to_string();
    }
    if squashed.chars().count() <= 24 {
        squashed
    } else {
        format!("{}...", squashed.chars().take(24).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::attachment_preview;

    #[test]
    fn attachment_preview_squashes_and_truncates() {
        let preview = attachment_preview("first line\nsecond line with extra detail");
        assert!(preview.contains("first line second line"));
        assert!(preview.ends_with("..."));
    }
}
