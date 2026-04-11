use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::ChatEntry;
use crate::ui::chat::{render_markdown_white, ACCENT, DIM, YELLOW};

// Claude Code style: ⏺ prefix on first line, indented continuation
pub(crate) fn render_assistant(lines: &mut Vec<Line<'static>>, entry: &ChatEntry) {
    if let Some(reasoning) = &entry.reasoning {
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

    let markdown = render_markdown_white(&entry.content);
    for (index, line) in markdown.into_iter().enumerate() {
        let mut spans = Vec::new();
        if index == 0 {
            spans.push(Span::styled("⏺ ", Style::default().fg(ACCENT)));
        } else {
            spans.push(Span::raw("  "));
        }
        spans.extend(line.spans);
        lines.push(Line::from(spans));
    }
}
