mod subagents;
mod tool_calls;
mod users;

use ratatui::style::{Color, Modifier};

use crate::app::{ChatEntry, ChatRole};

use self::subagents::render_subagent_call;
use self::tool_calls::render_tool_call;
use self::users::{render_assistant, render_user};

pub(crate) fn format_entry_as_strings(
    entry: &ChatEntry,
    all_entries: &[ChatEntry],
    index: usize,
) -> Vec<(String, ratatui::style::Style)> {
    let mut result: Vec<(String, ratatui::style::Style)> = Vec::new();
    let dim = ratatui::style::Style::default().fg(Color::Gray);
    let accent = ratatui::style::Style::default().fg(Color::LightMagenta);
    let cyan = ratatui::style::Style::default().fg(Color::Indexed(51));
    let white = ratatui::style::Style::default().fg(Color::Indexed(231));
    let red = ratatui::style::Style::default().fg(Color::LightRed);

    match &entry.role {
        ChatRole::User => render_user(entry, &mut result, cyan),
        ChatRole::Assistant => render_assistant(entry, &mut result, dim, white),
        ChatRole::ToolCall { id: tid, name } => {
            render_tool_call(entry, all_entries, index, tid, name, &mut result, dim, accent, red)
        }
        ChatRole::ToolResult { id: rid, .. } => {
            let has_preceding = index > 0
                && all_entries[..index].iter().rev().any(
                    |e| matches!(&e.role, ChatRole::ToolCall { id: ref tid, .. } if tid == rid),
                );
            if !has_preceding {
                result.push((
                    format!("  ⎿ {}", entry.content.lines().next().unwrap_or("")),
                    dim,
                ));
            }
        }
        ChatRole::Error => {
            let err_style = ratatui::style::Style::default()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD);
            result.push(("╭─ Error ──────────────────────────".to_string(), err_style));
            for line in entry.content.lines() {
                result.push((format!("│ {}", line), red));
            }
            result.push(("╰──────────────────────────────────".to_string(), err_style));
        }
        ChatRole::System => {
            if entry.content.is_empty() {
                result.push((String::new(), dim));
            } else {
                for line in entry.content.lines() {
                    result.push((format!("  {}", line), dim));
                }
            }
        }
        ChatRole::SubAgentCall { description } => {
            render_subagent_call(description, all_entries, index, &mut result, dim, accent);
        }
        ChatRole::SubAgentToolCall { .. } => {}
        ChatRole::SubAgentResult => {}
        ChatRole::AskUser { .. } => {}
    }
    result
}
