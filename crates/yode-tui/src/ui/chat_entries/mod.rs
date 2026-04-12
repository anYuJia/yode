mod assistant;
pub(crate) mod folding;
mod metadata;
mod plain_lines;
mod tool_helpers;
mod tools;

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::ChatEntry;
use crate::ui::chat::{CODE_BG, CYAN, GREEN, WHITE};

pub(super) use assistant::render_assistant;
pub(crate) use plain_lines::{assistant_plain_lines, user_plain_lines};
pub(super) use tools::{render_standalone_result, render_tool_call};

pub(super) fn render_user(lines: &mut Vec<Line<'static>>, entry: &ChatEntry) {
    let user_style = Style::default().fg(CYAN);
    let highlighted_style = Style::default().fg(WHITE).bg(CODE_BG);
    for (index, line) in user_plain_lines(entry).into_iter().enumerate() {
        let prefix_style = if index == 0 {
            Style::default().fg(GREEN).add_modifier(Modifier::BOLD)
        } else {
            user_style
        };
        let content_style = if line.highlight_code {
            highlighted_style
        } else if index == 0 {
            user_style.add_modifier(Modifier::BOLD)
        } else {
            user_style
        };

        lines.push(Line::from(vec![
            Span::styled(line.prefix.to_string(), prefix_style),
            Span::styled(line.content, content_style),
        ]));
    }
}
