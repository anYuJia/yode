use crate::app::rendering::{
    highlight_code_line, is_code_block_line, markdown_to_plain,
};
use crate::app::ChatEntry;

pub(super) fn render_user(
    entry: &ChatEntry,
    result: &mut Vec<(String, ratatui::style::Style)>,
    cyan: ratatui::style::Style,
) {
    let mut first = true;
    for line in entry.content.lines() {
        if first {
            result.push((
                format!("> {}", line),
                cyan.add_modifier(ratatui::style::Modifier::BOLD),
            ));
            first = false;
        } else {
            result.push((format!("  {}", line), cyan));
        }
    }
    if first {
        result.push((
            "> ".to_string(),
            cyan.add_modifier(ratatui::style::Modifier::BOLD),
        ));
    }
}

pub(super) fn render_assistant(
    entry: &ChatEntry,
    result: &mut Vec<(String, ratatui::style::Style)>,
    dim: ratatui::style::Style,
    white: ratatui::style::Style,
) {
    result.push((String::new(), dim));
    let processed = markdown_to_plain(&entry.content);
    if processed.trim().is_empty() {
        return;
    }
    let mut first = true;
    for line in processed.lines() {
        if line.trim().is_empty() {
            result.push((String::new(), dim));
            continue;
        }
        if first {
            result.push((format!("⏺ {}", line), white));
            first = false;
        } else if is_code_block_line(&line) {
            let highlighted = highlight_code_line(&line);
            result.push((format!("  {}", highlighted), ratatui::style::Style::default()));
        } else {
            result.push((format!("  {}", line), white));
        }
    }
}
