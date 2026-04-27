use std::time::Duration;

use crate::app::{ChatEntry, ChatRole};
use crate::tool_grouping::SubAgentBatch;

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
    if agent_duration.is_some() {
        result.push((
            format!(
                "  ⎿ Done ({} {}{})",
                sub_tools.len(),
                if sub_tools.len() == 1 {
                    "tool use"
                } else {
                    "tool uses"
                },
                agent_duration
                    .map(|duration| format!(" · {}", crate::app::format_duration(duration)))
                    .unwrap_or_default()
            ),
            dim,
        ));
    } else {
        result.push(("  ⎿ Running…".to_string(), dim));
    }
    if !sub_tools.is_empty() {
        result.push(("  (ctrl+o to expand)".to_string(), dim));
    }
}

pub(super) fn render_grouped_subagent_batch(
    all_entries: &[ChatEntry],
    batch: &SubAgentBatch,
    result: &mut Vec<(String, ratatui::style::Style)>,
    dim: ratatui::style::Style,
    accent: ratatui::style::Style,
) {
    let noun = grouped_subagent_title(batch);
    result.push((
        format!(
            "⏺ {} {} done (ctrl+o to expand)",
            batch.items.len(),
            noun
        ),
        accent,
    ));

    let max_items = 3;
    for (index, item) in batch.items.iter().take(max_items).enumerate() {
        let branch = if index + 1 == batch.items.len().min(max_items) {
            "  └─ "
        } else {
            "  ├─ "
        };
        let child_prefix = if index + 1 == batch.items.len().min(max_items) {
            "     "
        } else {
            "  │  "
        };
        result.push((
            format!(
                "{}{} · {}",
                branch,
                item.description,
                tool_use_label(item.tool_count)
            ),
            dim,
        ));
        let done_line = all_entries
            .get(item.result_index)
            .map(|entry| {
                if entry.content.trim().is_empty() {
                    "Done".to_string()
                } else {
                    "Done".to_string()
                }
            })
            .unwrap_or_else(|| "Done".to_string());
        result.push((format!("{}⎿  {}", child_prefix, done_line), dim));
    }

    if batch.items.len() > max_items {
        result.push((
            format!("     … +{} more agents", batch.items.len() - max_items),
            dim,
        ));
    }
}

fn grouped_subagent_title(batch: &SubAgentBatch) -> &'static str {
    let all_explore = batch.items.iter().all(|item| {
        item.description.to_lowercase().contains("explore") || item.description.contains("审查")
    });
    let all_plan = batch
        .items
        .iter()
        .all(|item| item.description.to_lowercase().contains("plan"));

    if all_explore {
        if batch.items.len() == 1 {
            "Explore agent"
        } else {
            "Explore agents"
        }
    } else if all_plan {
        if batch.items.len() == 1 {
            "Plan agent"
        } else {
            "Plan agents"
        }
    } else if batch.items.len() == 1 {
        "agent"
    } else {
        "agents"
    }
}

fn tool_use_label(count: usize) -> String {
    format!(
        "{} {}",
        count,
        if count == 1 { "tool use" } else { "tool uses" }
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ratatui::style::Style;

    use crate::app::{ChatEntry, ChatRole};
    use crate::tool_grouping::{SubAgentBatch, SubAgentBatchItem};

    use super::{render_grouped_subagent_batch, render_subagent_call};

    #[test]
    fn subagent_render_compacts_to_done_summary() {
        let mut result_entry = ChatEntry::new(ChatRole::SubAgentResult, "done".to_string());
        result_entry.duration = Some(Duration::from_secs(12));
        let entries = vec![
            ChatEntry::new(
                ChatRole::SubAgentCall {
                    description: "analyze repo".to_string(),
                },
                String::new(),
            ),
            ChatEntry::new(
                ChatRole::SubAgentToolCall {
                    name: "read_file".to_string(),
                },
                String::new(),
            ),
            result_entry,
        ];
        let mut rendered = Vec::new();
        render_subagent_call(
            "analyze repo",
            &entries,
            0,
            &mut rendered,
            Style::default(),
            Style::default(),
        );
        assert!(rendered[1].0.contains("Done (1 tool use"));
        assert!(rendered[2].0.contains("ctrl+o to expand"));
    }

    #[test]
    fn grouped_subagent_batch_compacts_multiple_segments() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::SubAgentCall {
                    description: "审查UI/UX分析遗漏".to_string(),
                },
                String::new(),
            ),
            ChatEntry::new(ChatRole::SubAgentResult, "done".to_string()),
            ChatEntry::new(
                ChatRole::SubAgentCall {
                    description: "审查交互与动效细节".to_string(),
                },
                String::new(),
            ),
            ChatEntry::new(ChatRole::SubAgentResult, "done".to_string()),
        ];
        let batch = SubAgentBatch {
            start_index: 0,
            next_index: 4,
            items: vec![
                SubAgentBatchItem {
                    call_index: 0,
                    result_index: 1,
                    description: "审查UI/UX分析遗漏".to_string(),
                    tool_count: 0,
                },
                SubAgentBatchItem {
                    call_index: 2,
                    result_index: 3,
                    description: "审查交互与动效细节".to_string(),
                    tool_count: 0,
                },
            ],
        };
        let mut rendered = Vec::new();
        render_grouped_subagent_batch(
            &entries,
            &batch,
            &mut rendered,
            Style::default(),
            Style::default(),
        );
        assert!(rendered[0].0.contains("2 Explore agents done"));
        assert!(rendered[1].0.contains("0 tool uses"));
    }
}
