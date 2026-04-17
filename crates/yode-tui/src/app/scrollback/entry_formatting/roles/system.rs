use ratatui::style::{Color, Style};

use crate::app::ChatEntry;
use crate::system_message::{parse_system_message, system_message_summary, SystemMessageKind};
use crate::tool_grouping::SystemBatch;

pub(super) fn render_system_entry(entry: &ChatEntry) -> Vec<(String, Style)> {
    let view = parse_system_message(&entry.content);
    if view.title.is_empty() {
        return vec![(String::new(), Style::default().fg(Color::Gray))];
    }

    let (prefix, title_style, detail_style) = system_styles(view.kind);
    let mut result = vec![(format!("{}{}", prefix, view.title), title_style)];
    for detail in view.detail_lines {
        result.push((format!("    {}", detail), detail_style));
    }
    result
}

pub(super) fn render_grouped_system_entries(
    all_entries: &[ChatEntry],
    batch: &SystemBatch,
) -> Vec<(String, Style)> {
    let mut result = vec![(
        format!("  ≡ {}({})", grouped_batch_title(batch), batch.items.len()),
        Style::default().fg(Color::Cyan),
    )];
    let max_items = 4;
    for (index, item) in batch.items.iter().take(max_items).enumerate() {
        let view = parse_system_message(&all_entries[item.entry_index].content);
        let (_, item_style, _) = system_styles(view.kind);
        let prefix = if index == 0 { "  ⎿  " } else { "     " };
        result.push((format!("{}{}", prefix, system_message_summary(&view)), item_style));
    }
    if batch.items.len() > max_items {
        result.push((
            format!("     … +{} more updates", batch.items.len() - max_items),
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

fn grouped_batch_title(batch: &SystemBatch) -> &'static str {
    if batch.items.iter().all(|item| item.kind == SystemMessageKind::Task) {
        "Task updates"
    } else if batch.items.iter().all(|item| item.kind == SystemMessageKind::Export) {
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
            "Session memory updated · summary · /tmp/live.md".to_string(),
        );
        let lines = render_system_entry(&entry);
        assert!(lines[0].0.contains("Session memory updated"));
        assert!(lines[1].0.contains("/tmp/live.md"));
    }

    #[test]
    fn scrollback_grouped_system_entries_render_batch_title() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::System,
                "Context compressed · auto · -4 msgs".to_string(),
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
        assert!(lines[1].0.contains("Context compressed"));
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
    }
}
