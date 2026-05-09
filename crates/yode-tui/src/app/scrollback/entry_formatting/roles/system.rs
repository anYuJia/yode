use ratatui::style::{Color, Style};

use crate::app::ChatEntry;
use crate::system_message::{
    format_system_detail_line, parse_system_message, system_message_summary, SystemMessageKind,
};
use crate::tool_grouping::SystemBatch;

pub(super) fn render_system_entry(entry: &ChatEntry, show_detail: bool) -> Vec<(String, Style)> {
    let view = parse_system_message(&entry.content);
    if view.title.is_empty() {
        return vec![(String::new(), Style::default().fg(Color::Gray))];
    }

    let (prefix, title_style, detail_style) = system_styles(view.kind);
    let compact_kind = matches!(
        view.kind,
        SystemMessageKind::Context
            | SystemMessageKind::Memory
            | SystemMessageKind::Export
            | SystemMessageKind::Lifecycle
            | SystemMessageKind::Plan
            | SystemMessageKind::Update
            | SystemMessageKind::Turn
            | SystemMessageKind::Task
    );
    let mut result = vec![(
        format!(
            "{}{}",
            prefix,
            if compact_kind {
                system_message_summary(&view)
            } else {
                view.title.clone()
            }
        ),
        title_style,
    )];
    if compact_kind && !show_detail {
        return result;
    }
    if compact_kind && matches!(view.kind, SystemMessageKind::Task | SystemMessageKind::Turn) {
        return result;
    }
    if show_detail {
        if let Some(first_detail) = view.detail_lines.first() {
            result.push((
                format!("    {}", format_system_detail_line(first_detail)),
                detail_style,
            ));
        }
        if view.detail_lines.len() > 1 {
            result.push((
                format!(
                    "    … +{} more lines (ctrl+o to inspect)",
                    view.detail_lines.len() - 1
                ),
                Style::default().fg(Color::Gray),
            ));
        }
        result.push((
            "    ctrl+o to inspect".to_string(),
            Style::default()
                .fg(Color::Gray)
                .add_modifier(ratatui::style::Modifier::ITALIC),
        ));
    }
    result
}

pub(super) fn render_grouped_system_entries(
    all_entries: &[ChatEntry],
    batch: &SystemBatch,
) -> Vec<(String, Style)> {
    let mut result = vec![(
        format!(
            "  ≡ {}({}) (ctrl+o to inspect)",
            grouped_batch_title(all_entries, batch),
            batch.items.len()
        ),
        Style::default().fg(Color::Cyan),
    )];
    let max_items = 3;
    let visible_items = batch
        .items
        .iter()
        .rev()
        .take(max_items)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    for (index, item) in visible_items.into_iter().enumerate() {
        let view = parse_system_message(&all_entries[item.entry_index].content);
        let (_, item_style, _) = system_styles(view.kind);
        let prefix = if index == 0 { "  ⎿  " } else { "     " };
        result.push((
            format!("{}{}", prefix, system_message_summary(&view)),
            item_style,
        ));
    }
    if batch.items.len() > max_items {
        result.push((
            format!("     … +{} earlier events", batch.items.len() - max_items),
            Style::default().fg(Color::Gray),
        ));
    }
    result
}

fn system_styles(kind: SystemMessageKind) -> (&'static str, Style, Style) {
    match kind {
        SystemMessageKind::Context => (
            "  ↺ ",
            Style::default().fg(Color::Yellow),
            Style::default().fg(Color::Gray),
        ),
        SystemMessageKind::Memory => (
            "  ≈ ",
            Style::default().fg(Color::Cyan),
            Style::default().fg(Color::Gray),
        ),
        SystemMessageKind::Budget => (
            "  ! ",
            Style::default().fg(Color::LightRed),
            Style::default().fg(Color::Yellow),
        ),
        SystemMessageKind::Export => (
            "  ↓ ",
            Style::default().fg(Color::LightGreen),
            Style::default().fg(Color::Gray),
        ),
        SystemMessageKind::Task => (
            "  ⧖ ",
            Style::default().fg(Color::Cyan),
            Style::default().fg(Color::Gray),
        ),
        SystemMessageKind::Turn => (
            "  ⚡ ",
            Style::default().fg(Color::LightGreen),
            Style::default().fg(Color::Gray),
        ),
        SystemMessageKind::Warning => (
            "  ! ",
            Style::default().fg(Color::Yellow),
            Style::default().fg(Color::Gray),
        ),
        SystemMessageKind::Lifecycle => (
            "  · ",
            Style::default().fg(Color::Cyan),
            Style::default().fg(Color::Gray),
        ),
        SystemMessageKind::Plan => (
            "  ≡ ",
            Style::default().fg(Color::Cyan),
            Style::default().fg(Color::Gray),
        ),
        SystemMessageKind::Update => (
            "  ↑ ",
            Style::default().fg(Color::Cyan),
            Style::default().fg(Color::Gray),
        ),
        SystemMessageKind::Generic => (
            "  · ",
            Style::default().fg(Color::Gray),
            Style::default().fg(Color::White),
        ),
    }
}

fn grouped_batch_title(all_entries: &[ChatEntry], batch: &SystemBatch) -> &'static str {
    if batch.items.iter().all(|item| {
        all_entries
            .get(item.entry_index)
            .is_some_and(|entry| entry.content.to_ascii_lowercase().contains("remote"))
    }) {
        return "Remote updates";
    }
    if batch.items.iter().all(|item| {
        all_entries
            .get(item.entry_index)
            .is_some_and(|entry| entry.content.to_ascii_lowercase().contains("review"))
    }) {
        return "Review artifacts";
    }
    if batch.items.iter().all(|item| {
        all_entries
            .get(item.entry_index)
            .is_some_and(|entry| entry.content.to_ascii_lowercase().contains("workflow"))
    }) {
        return "Workflow artifacts";
    }
    if batch
        .items
        .iter()
        .all(|item| item.kind == SystemMessageKind::Task)
    {
        if batch.items.iter().all(|item| {
            all_entries
                .get(item.entry_index)
                .is_some_and(|entry| entry.content.to_ascii_lowercase().contains("hook"))
        }) {
            "Hook updates"
        } else {
            "Task updates"
        }
    } else if batch
        .items
        .iter()
        .all(|item| item.kind == SystemMessageKind::Export)
    {
        "Exports"
    } else {
        "Status updates"
    }
}

#[cfg(test)]
mod tests {
    use crate::app::{ChatEntry, ChatRole};

    use crate::tool_grouping::{SystemBatch, SystemBatchItem};

    use super::{render_grouped_system_entries, render_system_entry};

    #[test]
    fn scrollback_system_entry_uses_compact_title_line() {
        let entry = ChatEntry::new(
            ChatRole::System,
            "Session memory updated · summary · /tmp/live.md\nnote · older context trimmed"
                .to_string(),
        );
        let lines = render_system_entry(&entry, true);
        assert!(lines[0].0.contains("Session memory updated"));
        assert!(lines[1].0.contains("/tmp/live.md"));
        assert!(lines[2].0.contains("+1 more lines"));
        assert!(lines[3].0.contains("ctrl+o to inspect"));
    }

    #[test]
    fn scrollback_grouped_system_entries_render_batch_title() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::System,
                "Context compacted · auto · -4 msgs".to_string(),
            ),
            ChatEntry::new(
                ChatRole::System,
                "Session memory updated · summary · /tmp/live.md".to_string(),
            ),
        ];
        let batch = SystemBatch {
            start_index: 0,
            next_index: 2,
            items: vec![
                SystemBatchItem {
                    entry_index: 0,
                    kind: crate::system_message::SystemMessageKind::Context,
                },
                SystemBatchItem {
                    entry_index: 1,
                    kind: crate::system_message::SystemMessageKind::Memory,
                },
            ],
        };
        let lines = render_grouped_system_entries(&entries, &batch);
        assert!(lines[0].0.contains("Status updates(2)"));
        assert!(lines[0].0.contains("ctrl+o to inspect"));
        assert!(lines[1].0.contains("Context compacted"));
    }

    #[test]
    fn scrollback_grouped_system_entries_use_task_batch_title() {
        let entries = vec![
            ChatEntry::new(ChatRole::System, "[Task:info] first".to_string()),
            ChatEntry::new(ChatRole::System, "[Task:warn] second".to_string()),
        ];
        let batch = SystemBatch {
            start_index: 0,
            next_index: 2,
            items: vec![
                SystemBatchItem {
                    entry_index: 0,
                    kind: crate::system_message::SystemMessageKind::Task,
                },
                SystemBatchItem {
                    entry_index: 1,
                    kind: crate::system_message::SystemMessageKind::Task,
                },
            ],
        };
        let lines = render_grouped_system_entries(&entries, &batch);
        assert!(lines[0].0.contains("Task updates(2)"));
        assert!(lines[0].0.contains("ctrl+o to inspect"));
    }

    #[test]
    fn scrollback_grouped_system_entries_use_hook_batch_title() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::System,
                "[Task:warn] hook timeout: scripts/pre-tool".to_string(),
            ),
            ChatEntry::new(
                ChatRole::System,
                "[Task:info] hook deferred: preview".to_string(),
            ),
        ];
        let batch = SystemBatch {
            start_index: 0,
            next_index: 2,
            items: vec![
                SystemBatchItem {
                    entry_index: 0,
                    kind: crate::system_message::SystemMessageKind::Task,
                },
                SystemBatchItem {
                    entry_index: 1,
                    kind: crate::system_message::SystemMessageKind::Task,
                },
            ],
        };
        let lines = render_grouped_system_entries(&entries, &batch);
        assert!(lines[0].0.contains("Hook updates(2)"));
    }

    #[test]
    fn scrollback_grouped_system_entries_prefer_latest_items_when_trimming() {
        let entries = vec![
            ChatEntry::new(ChatRole::System, "Session resumed.".to_string()),
            ChatEntry::new(
                ChatRole::System,
                "Context compacted · auto · -4 msgs".to_string(),
            ),
            ChatEntry::new(
                ChatRole::System,
                "Session memory updated · summary · /tmp/live.md".to_string(),
            ),
            ChatEntry::new(
                ChatRole::System,
                "Diagnostics bundle exported to: /tmp/bundle".to_string(),
            ),
        ];
        let batch = SystemBatch {
            start_index: 0,
            next_index: 4,
            items: vec![
                SystemBatchItem {
                    entry_index: 0,
                    kind: crate::system_message::SystemMessageKind::Update,
                },
                SystemBatchItem {
                    entry_index: 1,
                    kind: crate::system_message::SystemMessageKind::Context,
                },
                SystemBatchItem {
                    entry_index: 2,
                    kind: crate::system_message::SystemMessageKind::Memory,
                },
                SystemBatchItem {
                    entry_index: 3,
                    kind: crate::system_message::SystemMessageKind::Export,
                },
            ],
        };
        let lines = render_grouped_system_entries(&entries, &batch);
        let rendered = lines
            .iter()
            .map(|(line, _)| line.clone())
            .collect::<Vec<_>>();
        assert!(rendered
            .iter()
            .any(|line| line.contains("Context compacted")));
        assert!(rendered
            .iter()
            .any(|line| line.contains("Session memory updated")));
        assert!(rendered
            .iter()
            .any(|line| line.contains("Diagnostics bundle exported")));
        assert!(rendered
            .iter()
            .all(|line| !line.contains("Session resumed.")));
        assert!(rendered
            .iter()
            .any(|line| line.contains("+1 earlier events")));
    }

    #[test]
    fn scrollback_grouped_system_entries_name_remote_review_and_workflow_batches() {
        for (needle, expected) in [
            ("Remote live session ready", "Remote updates"),
            ("Review artifact exported", "Review artifacts"),
            ("Workflow execution artifact", "Workflow artifacts"),
        ] {
            let entries = vec![
                ChatEntry::new(ChatRole::System, needle.to_string()),
                ChatEntry::new(ChatRole::System, format!("{} again", needle)),
            ];
            let batch = SystemBatch {
                start_index: 0,
                next_index: 2,
                items: vec![
                    SystemBatchItem {
                        entry_index: 0,
                        kind: crate::system_message::SystemMessageKind::Generic,
                    },
                    SystemBatchItem {
                        entry_index: 1,
                        kind: crate::system_message::SystemMessageKind::Generic,
                    },
                ],
            };
            let lines = render_grouped_system_entries(&entries, &batch);
            assert!(lines[0].0.contains(expected));
        }
    }

    #[test]
    fn scrollback_turn_summary_entry_renders_detail_lines() {
        let entry = ChatEntry::new(
            ChatRole::System,
            "Turn completed · 1.4s · 3 tools · 1.2k↑ 180↓ tok\nsession · 15.4k total tok · 34 tools"
                .to_string(),
        );
        let lines = render_system_entry(&entry, true);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].0.contains("Turn completed · 1.4s · 3 tools"));
    }

    #[test]
    fn scrollback_task_notification_is_single_line_summary() {
        let entry = ChatEntry::new(
            ChatRole::System,
            "<task-notification>\n<task-id>task-5</task-id>\n<status>completed</status>\n<summary>completed: Explore Yode project deeply</summary>\n<output-path>/tmp/out</output-path>\n<result>mapped rust workspace project</result>\n</task-notification>"
                .to_string(),
        );
        let lines = render_system_entry(&entry, true);
        assert_eq!(lines.len(), 1);
        assert!(lines[0]
            .0
            .contains("Task task-5 completed · Explore Yode project deeply"));
        assert!(!lines[0].0.contains("/tmp/out"));
    }
}
