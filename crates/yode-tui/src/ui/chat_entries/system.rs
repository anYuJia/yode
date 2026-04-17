use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::ChatEntry;
use crate::system_message::{parse_system_message, system_message_summary, SystemMessageKind};
use crate::tool_grouping::SystemBatch;
use crate::ui::palette::{ERROR_COLOR, INFO_COLOR, LIGHT, MUTED, SUCCESS_COLOR, WARNING_COLOR};

pub(crate) fn render_system_entry(lines: &mut Vec<Line<'static>>, entry: &ChatEntry) {
    let view = parse_system_message(&entry.content);
    if view.title.is_empty() {
        return;
    }

    let (prefix, title_style, detail_style) = system_styles(view.kind);
    lines.push(Line::from(vec![
        Span::styled(prefix.to_string(), title_style),
        Span::styled(view.title, title_style.add_modifier(Modifier::BOLD)),
    ]));

    for detail in view.detail_lines {
        lines.push(Line::from(vec![
            Span::styled("    ".to_string(), Style::default().fg(MUTED)),
            Span::styled(detail, detail_style),
        ]));
    }
}

pub(crate) fn render_grouped_system_entries(
    lines: &mut Vec<Line<'static>>,
    all_entries: &[ChatEntry],
    batch: &SystemBatch,
) {
    lines.push(Line::from(vec![
        Span::styled(
            "  ≡ ".to_string(),
            Style::default().fg(INFO_COLOR).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}({})", grouped_batch_title(batch), batch.items.len()),
            Style::default().fg(LIGHT).add_modifier(Modifier::BOLD),
        ),
    ]));

    let max_items = 4;
    for (index, item) in batch.items.iter().take(max_items).enumerate() {
        let entry = &all_entries[item.entry_index];
        let view = parse_system_message(&entry.content);
        let (_, item_style, _) = system_styles(view.kind);
        let prefix = if index == 0 { "  ⎿  " } else { "     " };
        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, system_message_summary(&view)),
            item_style,
        )));
    }

    if batch.items.len() > max_items {
        lines.push(Line::from(Span::styled(
            format!("     … +{} more updates", batch.items.len() - max_items),
            Style::default().fg(MUTED),
        )));
    }
}

fn system_styles(kind: SystemMessageKind) -> (&'static str, Style, Style) {
    match kind {
        SystemMessageKind::Context => (
            "  ↺ ",
            Style::default().fg(WARNING_COLOR),
            Style::default().fg(MUTED),
        ),
        SystemMessageKind::Memory => (
            "  ≈ ",
            Style::default().fg(INFO_COLOR),
            Style::default().fg(MUTED),
        ),
        SystemMessageKind::Budget => (
            "  ! ",
            Style::default().fg(ERROR_COLOR),
            Style::default().fg(WARNING_COLOR),
        ),
        SystemMessageKind::Export => (
            "  ↓ ",
            Style::default().fg(SUCCESS_COLOR),
            Style::default().fg(MUTED),
        ),
        SystemMessageKind::Task => (
            "  ⧖ ",
            Style::default().fg(INFO_COLOR),
            Style::default().fg(MUTED),
        ),
        SystemMessageKind::Warning => (
            "  ! ",
            Style::default().fg(WARNING_COLOR),
            Style::default().fg(MUTED),
        ),
        SystemMessageKind::Lifecycle => (
            "  · ",
            Style::default().fg(INFO_COLOR),
            Style::default().fg(MUTED),
        ),
        SystemMessageKind::Plan => (
            "  ≡ ",
            Style::default().fg(INFO_COLOR),
            Style::default().fg(MUTED),
        ),
        SystemMessageKind::Update => (
            "  ↑ ",
            Style::default().fg(INFO_COLOR),
            Style::default().fg(MUTED),
        ),
        SystemMessageKind::Generic => (
            "  · ",
            Style::default().fg(MUTED),
            Style::default().fg(LIGHT),
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
    fn render_system_entry_splits_title_and_detail_lines() {
        let entry = ChatEntry::new(
            ChatRole::System,
            "Context compressed · auto · -4 msgs\nsummary · older turns".to_string(),
        );
        let mut lines = Vec::new();
        render_system_entry(&mut lines, &entry);
        assert_eq!(lines.len(), 3);
        assert!(lines[0].to_string().contains("Context compressed"));
        assert!(lines[1].to_string().contains("auto · -4 msgs"));
        assert!(lines[2].to_string().contains("summary · older turns"));
    }

    #[test]
    fn render_grouped_system_entries_renders_batch_title() {
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
        let mut lines = Vec::new();
        render_grouped_system_entries(&mut lines, &entries, &batch);
        assert!(lines[0].to_string().contains("Status updates(2)"));
        assert!(lines[1].to_string().contains("Context compressed"));
    }

    #[test]
    fn render_grouped_system_entries_uses_task_batch_title() {
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
        let mut lines = Vec::new();
        render_grouped_system_entries(&mut lines, &entries, &batch);
        assert!(lines[0].to_string().contains("Task updates(2)"));
    }
}
