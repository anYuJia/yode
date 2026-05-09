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
    let mut result_status = "Done".to_string();
    for entry in &all_entries[index + 1..] {
        match &entry.role {
            ChatRole::SubAgentToolCall { name } => sub_tools.push(name.clone()),
            ChatRole::SubAgentResult => {
                agent_duration = entry.duration;
                result_status = subagent_status_text(&entry.content);
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

    if agent_duration.is_some() {
        result.push((
            format!(
                "⏺ {}({}){} · {} · {} {}",
                agent_type,
                description,
                timing,
                result_status,
                sub_tools.len(),
                if sub_tools.len() == 1 {
                    "tool use"
                } else {
                    "tool uses"
                }
            ),
            if result_status == "Timed out" {
                dim.fg(ratatui::style::Color::Yellow)
            } else {
                accent
            },
        ));
    } else {
        result.push((
            format!("⏺ {}({}){}", agent_type, description, timing),
            accent,
        ));
        result.push(("  ⎿ Running…".to_string(), dim));
        if !sub_tools.is_empty() {
            result.push(("  (ctrl+o to expand)".to_string(), dim));
        }
    }
}

pub(super) fn render_grouped_subagent_batch(
    all_entries: &[ChatEntry],
    batch: &SubAgentBatch,
    result: &mut Vec<(String, ratatui::style::Style)>,
    _dim: ratatui::style::Style,
    accent: ratatui::style::Style,
) {
    let noun = grouped_subagent_title(batch);
    let timed_out = batch
        .items
        .iter()
        .filter(|item| {
            item.result_index
                .and_then(|index| all_entries.get(index))
                .is_some_and(|entry| is_timeout_text(&entry.content))
        })
        .count();
    result.push((
        if batch.is_active {
            format!(
                "⏺ Running {} {}… (ctrl+o to expand)",
                batch.items.len(),
                noun
            )
        } else if timed_out > 0 {
            format!(
                "⏺ {} {} finished ({} timed out) (ctrl+o to expand)",
                batch.items.len(),
                noun,
                timed_out
            )
        } else {
            format!(
                "⏺ {} {} finished (ctrl+o to expand)",
                batch.items.len(),
                noun
            )
        },
        accent,
    ));

    for (index, item) in batch.items.iter().enumerate() {
        let is_last = index + 1 == batch.items.len();
        let branch = if is_last { "   └─ " } else { "   ├─ " };
        let child_prefix = if is_last { "      " } else { "   │  " };
        result.push((
            format!(
                "{}{} · {} {}",
                branch,
                item.description,
                item.tool_count,
                if item.tool_count == 1 {
                    "tool use"
                } else {
                    "tool uses"
                }
            ),
            accent,
        ));
        let status = item
            .result_index
            .and_then(|result_index| all_entries.get(result_index))
            .map(|entry| subagent_status_text(&entry.content))
            .unwrap_or_else(|| "Initializing…".to_string());
        result.push((format!("{}⎿  {}", child_prefix, status), accent));
    }
}

fn grouped_subagent_title(batch: &SubAgentBatch) -> &'static str {
    let all_explore = batch
        .items
        .iter()
        .all(|item| is_explore_agent_description(&item.description));
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

fn is_explore_agent_description(description: &str) -> bool {
    let lower = description.to_lowercase();
    lower.contains("explore")
        || lower.contains("analyze")
        || lower.contains("analyse")
        || lower.contains("find")
        || lower.contains("inspect")
        || description.contains("审查")
        || description.contains("分析")
}

fn is_timeout_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("timed out") || lower.contains("timeout")
}

fn subagent_status_text(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        "Timed out".to_string()
    } else if lower.contains("failed") || lower.contains("error") {
        "Failed".to_string()
    } else {
        "Done".to_string()
    }
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
        assert_eq!(rendered.len(), 1);
        assert!(rendered[0].0.contains("Done"));
        assert!(rendered[0].0.contains("1 tool use"));
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
            is_active: false,
            items: vec![
                SubAgentBatchItem {
                    call_index: 0,
                    result_index: Some(1),
                    description: "审查UI/UX分析遗漏".to_string(),
                    tool_count: 0,
                },
                SubAgentBatchItem {
                    call_index: 2,
                    result_index: Some(3),
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
        assert!(rendered[0].0.contains("2 Explore agents finished"));
        assert!(rendered[1].0.contains("0 tool uses"));
        assert!(rendered[2].0.contains("Done"));
    }

    #[test]
    fn grouped_subagent_batch_renders_active_explore_agents_tree() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::SubAgentCall {
                    description: "Analyze Yode architecture".to_string(),
                },
                String::new(),
            ),
            ChatEntry::new(
                ChatRole::SubAgentCall {
                    description: "Find claude-code-rev project".to_string(),
                },
                String::new(),
            ),
        ];
        let batch = SubAgentBatch {
            start_index: 0,
            next_index: 2,
            is_active: true,
            items: vec![
                SubAgentBatchItem {
                    call_index: 0,
                    result_index: None,
                    description: "Analyze Yode architecture".to_string(),
                    tool_count: 0,
                },
                SubAgentBatchItem {
                    call_index: 1,
                    result_index: None,
                    description: "Find claude-code-rev project".to_string(),
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
        assert!(rendered[0].0.contains("Running 2 Explore agents"));
        assert!(rendered[1].0.contains("Analyze Yode architecture"));
        assert!(rendered[2].0.contains("Initializing"));
        assert!(rendered[3].0.contains("Find claude-code-rev project"));
    }

    #[test]
    fn subagent_render_surfaces_timeout_status() {
        let mut result_entry = ChatEntry::new(
            ChatRole::SubAgentResult,
            "timed out\n… +2 more lines".to_string(),
        );
        result_entry.duration = Some(Duration::from_secs(30));
        let entries = vec![
            ChatEntry::new(
                ChatRole::SubAgentCall {
                    description: "Explore yode core modules".to_string(),
                },
                String::new(),
            ),
            result_entry,
        ];
        let mut rendered = Vec::new();
        render_subagent_call(
            "Explore yode core modules",
            &entries,
            0,
            &mut rendered,
            Style::default(),
            Style::default(),
        );
        assert!(rendered[0].0.contains("Timed out"));
    }
}
