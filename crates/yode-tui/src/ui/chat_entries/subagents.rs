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
    for entry in &all_entries[index + 1..] {
        match &entry.role {
            crate::app::ChatRole::SubAgentToolCall { .. } => tool_count += 1,
            crate::app::ChatRole::SubAgentResult => {
                done = true;
                break;
            }
            _ => break,
        }
    }

    lines.push(Line::from(vec![
        Span::styled("⏺ ", Style::default().fg(ACCENT)),
        Span::styled(description.to_string(), Style::default().fg(WHITE)),
    ]));
    lines.push(Line::from(Span::styled(
        format!(
            "  ⎿  {} ({})",
            if done { "Done" } else { "Running…" },
            tool_use_label(tool_count)
        ),
        Style::default().fg(DIM),
    )));
    if tool_count > 0 {
        lines.push(Line::from(Span::styled(
            "  (ctrl+o to expand)",
            Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
        )));
    }
}

pub(crate) fn render_grouped_subagent_batch(
    lines: &mut Vec<Line<'static>>,
    all_entries: &[ChatEntry],
    batch: &SubAgentBatch,
) {
    lines.push(Line::from(vec![
        Span::styled("⏺ ", Style::default().fg(ACCENT)),
        Span::styled(
            format!(
                "{} {} finished (ctrl+o to expand)",
                batch.items.len(),
                grouped_subagent_title(batch)
            ),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
    ]));

    let max_items = 2;
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
        lines.push(Line::from(Span::styled(
            format!(
                "{}{} · {}",
                branch,
                item.description,
                tool_use_label(item.tool_count)
            ),
            Style::default().fg(DIM),
        )));
        let done_line = all_entries
            .get(item.result_index)
            .map(|_| "Done")
            .unwrap_or("Done");
        lines.push(Line::from(Span::styled(
            format!("{}⎿  {}", child_prefix, done_line),
            Style::default().fg(DIM),
        )));
    }

    if batch.items.len() > max_items {
        lines.push(Line::from(Span::styled(
            format!("     … +{} more agents", batch.items.len() - max_items),
            Style::default().fg(DIM),
        )));
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
        assert!(lines[1].to_string().contains("Done (1 tool use)"));
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
        let mut lines = Vec::new();
        render_grouped_subagent_batch(&mut lines, &entries, &batch);
        assert!(lines[0].to_string().contains("2 Explore agents finished"));
        assert!(lines[1].to_string().contains("0 tool uses"));
    }
}
