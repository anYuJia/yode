use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use crate::ui::palette::{BORDER_MUTED, LIGHT, MUTED, SELECT_ACCENT, SELECT_BG, SURFACE_BG};

pub(super) const COMPLETION_BG: Color = SURFACE_BG;
pub(super) const COMPLETION_SELECTED_BG: Color = SELECT_BG;
pub(super) const COMPLETION_SELECTED_FG: Color = LIGHT;

pub(super) fn truncate_ellipsis(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    format!("{}…", text.chars().take(max_chars.saturating_sub(1)).collect::<String>())
}

pub(super) fn completion_candidate_line(
    selected: bool,
    command: &str,
    description: &str,
    command_width: usize,
    available_width: usize,
) -> Line<'static> {
    let desc_max = available_width.saturating_sub(command_width + 7).max(1);
    let desc_truncated = truncate_ellipsis(description, desc_max);
    let bg = COMPLETION_BG;

    if selected {
        Line::from(vec![
            Span::styled(
                " ❯",
                Style::default()
                    .fg(SELECT_ACCENT)
                    .bg(COMPLETION_SELECTED_BG)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<width$}", command, width = command_width),
                Style::default()
                    .fg(COMPLETION_SELECTED_FG)
                    .bg(COMPLETION_SELECTED_BG)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " │ ",
                Style::default().fg(BORDER_MUTED).bg(COMPLETION_SELECTED_BG),
            ),
            Span::styled(
                format!("{} ", desc_truncated),
                Style::default()
                    .fg(MUTED)
                    .bg(COMPLETION_SELECTED_BG),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled("  ", Style::default().bg(bg)),
            Span::styled(
                format!("{:<width$}", command, width = command_width),
                Style::default().fg(LIGHT).bg(bg),
            ),
            Span::styled(" │ ", Style::default().fg(BORDER_MUTED).bg(bg)),
            Span::styled(
                format!("{} ", desc_truncated),
                Style::default().fg(MUTED).bg(bg),
            ),
        ])
    }
}
