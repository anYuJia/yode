use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::ChatEntry;
use crate::tool_grouping::SubAgentBatch;
use crate::ui::chat::{ACCENT, DIM, WHITE};

pub(crate) fn render_subagent_call(
    lines: &mut Vec<Line<'static>>,
    description: &str,
    all_entries: &[ChatEntry],
    index: usize,
) {
    let mut tool_count = 0usize;
    let mut done = false;
    let mut status = "Done".to_string();
    for entry in &all_entries[index + 1..] {
        match &entry.role {
            crate::app::ChatRole::SubAgentToolCall { .. } => tool_count += 1,
            crate::app::ChatRole::SubAgentResult => {
                done = true;
                status = subagent_status_text(&entry.content);
                break;
            }
            _ => break,
        }
    }

    if done {
        lines.push(Line::from(vec![
            Span::styled("⏺ ", Style::default().fg(ACCENT)),
            Span::styled(description.to_string(), Style::default().fg(WHITE)),
            Span::styled(
                format!(" · {} · {}", status, tool_use_label(tool_count)),
                Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
            ),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("⏺ ", Style::default().fg(ACCENT)),
            Span::styled(description.to_string(), Style::default().fg(WHITE)),
        ]));
        lines.push(Line::from(Span::styled(
            format!("  ⎿  Running… ({})", tool_use_label(tool_count)),
            Style::default().fg(DIM),
        )));
        if tool_count > 0 {
            lines.push(Line::from(Span::styled(
                "  (ctrl+o to expand)",
                Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
            )));
        }
    }
}

pub(crate) fn render_grouped_subagent_batch(
    lines: &mut Vec<Line<'static>>,
    all_entries: &[ChatEntry],
    batch: &SubAgentBatch,
) {
    let timed_out = batch
        .items
        .iter()
        .filter(|item| {
            item.result_index
                .and_then(|index| all_entries.get(index))
                .is_some_and(|entry| is_timeout_text(&entry.content))
        })
        .count();
    lines.push(Line::from(vec![
        Span::styled("⏺ ", Style::default().fg(ACCENT)),
        Span::styled(
            if batch.is_active {
                format!(
                    "Running {} {}… (ctrl+o to expand)",
                    batch.items.len(),
                    grouped_subagent_title(batch)
                )
            } else if timed_out > 0 {
                format!(
                    "{} {} finished ({} timed out) (ctrl+o to expand)",
                    batch.items.len(),
                    grouped_subagent_title(batch),
                    timed_out
                )
            } else {
                format!(
                    "{} {} finished (ctrl+o to expand)",
                    batch.items.len(),
                    grouped_subagent_title(batch)
                )
            },
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
    ]));

    for (index, item) in batch.items.iter().enumerate() {
        let is_last = index + 1 == batch.items.len();
        let branch = if is_last { "   └─ " } else { "   ├─ " };
        let child_prefix = if is_last { "      " } else { "   │  " };
        lines.push(Line::from(vec![
            Span::styled(branch.to_string(), Style::default().fg(DIM)),
            Span::styled(item.description.clone(), Style::default().fg(WHITE)),
            Span::styled(
                format!(" · {}", tool_use_label(item.tool_count)),
                Style::default().fg(DIM),
            ),
        ]));
        let status = item
            .result_index
            .and_then(|result_index| all_entries.get(result_index))
            .map(|entry| subagent_status_text(&entry.content))
            .unwrap_or_else(|| "Initializing…".to_string());
        lines.push(Line::from(Span::styled(
            format!("{}⎿  {}", child_prefix, status),
            Style::default().fg(DIM),
        )));
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

fn tool_use_label(count: usize) -> String {
    format!(
        "{} {}",
        count,
        if count == 1 { "tool use" } else { "tool uses" }
    )
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
    use crate::app::{ChatEntry, ChatRole};
    use crate::tool_grouping::{SubAgentBatch, SubAgentBatchItem};

    use super::{render_grouped_subagent_batch, render_subagent_call};

    #[test]
    fn renders_single_subagent_compact_summary() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::SubAgentCall {
                    description: "审查UI/UX分析遗漏".to_string(),
                },
                String::new(),
            ),
            ChatEntry::new(
                ChatRole::SubAgentToolCall {
                    name: "read_file".to_string(),
                },
                String::new(),
            ),
            ChatEntry::new(ChatRole::SubAgentResult, "done".to_string()),
        ];
        let mut lines = Vec::new();
        render_subagent_call(&mut lines, "审查UI/UX分析遗漏", &entries, 0);
        assert!(lines[0].to_string().contains("审查UI/UX分析遗漏"));
        assert!(lines[0].to_string().contains("Done · 1 tool use"));
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn renders_grouped_subagent_batch_summary() {
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
        let mut lines = Vec::new();
        render_grouped_subagent_batch(&mut lines, &entries, &batch);
        assert!(lines[0].to_string().contains("2 Explore agents finished"));
        assert!(lines[1].to_string().contains("0 tool uses"));
        assert!(lines[2].to_string().contains("Done"));
    }

    #[test]
    fn renders_active_grouped_explore_agents_tree() {
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
        let mut lines = Vec::new();
        render_grouped_subagent_batch(&mut lines, &entries, &batch);
        assert!(lines[0].to_string().contains("Running 2 Explore agents"));
        assert!(lines[1].to_string().contains("Analyze Yode architecture"));
        assert!(lines[2].to_string().contains("Initializing"));
        assert!(lines[3]
            .to_string()
            .contains("Find claude-code-rev project"));
    }
}
