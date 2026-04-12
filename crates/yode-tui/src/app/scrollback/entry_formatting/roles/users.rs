use crate::app::ChatEntry;
use crate::app::rendering::highlight_code_line;
use crate::ui::chat_entries::{assistant_plain_lines, user_plain_lines};

pub(super) fn render_user(
    entry: &ChatEntry,
    result: &mut Vec<(String, ratatui::style::Style)>,
    cyan: ratatui::style::Style,
) {
    for (index, line) in user_plain_lines(entry).into_iter().enumerate() {
        let style = if index == 0 {
            cyan.add_modifier(ratatui::style::Modifier::BOLD)
        } else {
            cyan
        };
        let content = if line.highlight_code {
            highlight_code_line(&line.content)
        } else {
            line.content
        };
        result.push((format!("{}{}", line.prefix, content), style));
    }
}

pub(super) fn render_assistant(
    entry: &ChatEntry,
    result: &mut Vec<(String, ratatui::style::Style)>,
    dim: ratatui::style::Style,
    white: ratatui::style::Style,
) {
    result.push((String::new(), dim));
    let lines = assistant_plain_lines(entry);
    if lines.is_empty() {
        return;
    }
    for line in lines {
        if line.content.is_empty() {
            result.push((String::new(), dim));
            continue;
        }
        if line.highlight_code {
            result.push((
                format!("{}{}", line.prefix, highlight_code_line(&line.content)),
                ratatui::style::Style::default(),
            ));
        } else {
            result.push((format!("{}{}", line.prefix, line.content), white));
        }
    }
}
