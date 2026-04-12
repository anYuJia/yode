mod assistant;
mod tool_helpers;
mod tools;

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::ChatEntry;
use crate::ui::chat::{CYAN, GREEN};

pub(super) use assistant::render_assistant;
pub(super) use tools::{render_standalone_result, render_tool_call};

pub(super) fn render_user(lines: &mut Vec<Line<'static>>, entry: &ChatEntry) {
    let user_style = Style::default().fg(CYAN);
    for (index, line) in entry.content.lines().enumerate() {
        if index == 0 {
            lines.push(Line::from(vec![
                Span::styled(
                    "> ",
                    Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
                ),
                Span::styled(line.to_string(), user_style.add_modifier(Modifier::BOLD)),
            ]));
        } else {
            lines.push(Line::from(Span::styled(format!("  {}", line), user_style)));
        }
    }
}
