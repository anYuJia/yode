use std::time::Duration;

use crate::app::rendering::capitalize;
use crate::app::{ChatEntry, ChatRole};

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

    result.push((format!("⏺ {}({}){}", agent_type, description, timing), accent));

    let max_show = 3;
    let total = sub_tools.len();
    for (index, tool_name) in sub_tools.iter().enumerate() {
        if index >= max_show {
            result.push((
                format!("     … +{} more tool uses (ctrl+o to expand)", total - max_show),
                dim,
            ));
            break;
        }
        let prefix = if index == 0 { "  ⎿  " } else { "     " };
        result.push((format!("{}{}(…)", prefix, capitalize(tool_name)), dim));
    }
    if total == 0 {
        result.push(("  ⎿  (no tool calls)".to_string(), dim));
    }
}
