use std::time::Duration;

use crate::app::{ChatEntry, ChatRole};
use crate::ui::chat_entries::folding::fold_subagent_tool_calls;

pub(super) fn render_subagent_call(
    description: &str,
    all_entries: &[ChatEntry],
    index: usize,
    result: &mut Vec<(String, ratatui::style::Style)>,
    dim: ratatui::style::Style,
    accent: ratatui::style::Style,
) {
    let mut sub_tools: Vec<String> = Vec::new();
    let mut agent_duration: Option<Duration> = None;
    for entry in &all_entries[index + 1..] {
        match &entry.role {
            ChatRole::SubAgentToolCall { name } => sub_tools.push(name.clone()),
            ChatRole::SubAgentResult => {
                agent_duration = entry.duration;
                break;
            }
            _ => break,
        }
    }

    let agent_type = if description.to_lowercase().contains("explore") {
        "Explore"
    } else if description.to_lowercase().contains("plan") {
        "Plan"
    } else {
        "Agent"
    };

    let timing = agent_duration
        .map(|duration| format!(" ── {}", crate::app::format_duration(duration)))
        .unwrap_or_default();

    result.push((
        format!("⏺ {}({}){}", agent_type, description, timing),
        accent,
    ));

    for line in fold_subagent_tool_calls(&sub_tools, 3) {
        result.push((line, dim));
    }
}
