use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::ChatEntry;
use crate::system_message::{
    format_system_detail_line, parse_system_message, system_message_summary, SystemMessageKind,
};
use crate::tool_grouping::SystemBatch;
use crate::ui::chat::render_markdown_white_with_options;
use crate::ui::palette::{ERROR_COLOR, INFO_COLOR, LIGHT, MUTED, SUCCESS_COLOR, WARNING_COLOR};

pub(crate) fn render_system_entry(lines: &mut Vec<Line<'static>>, entry: &ChatEntry) {
    let view = parse_system_message(&entry.content);
    if view.title.is_empty() {
        return;
    }

    let (prefix, title_style, _detail_style) = system_styles(view.kind);
    lines.push(Line::from(vec![
        Span::styled(prefix.to_string(), title_style),
        Span::styled(view.title, title_style.add_modifier(Modifier::BOLD)),
    ]));

    for detail in view.detail_lines {
        for line in
            render_markdown_white_with_options(&format_system_detail_line(&detail), None, true)
        {
            let mut spans = vec![Span::styled("    ".to_string(), Style::default().fg(MUTED))];
            spans.extend(line.spans);
            lines.push(Line::from(spans));
        }
    }
    lines.push(Line::from(vec![
        Span::styled("    ".to_string(), Style::default().fg(MUTED)),
        Span::styled(
            "ctrl+o to inspect",
            Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
        ),
    ]));
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
            format!(
                "{}({})",
                grouped_batch_title(all_entries, batch),
                batch.items.len()
            ),
            Style::default().fg(LIGHT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " (ctrl+o to inspect)".to_string(),
            Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
        ),
    ]));

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
            format!("     … +{} earlier events", batch.items.len() - max_items),
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
        SystemMessageKind::Turn => (
            "  ⚡ ",
            Style::default().fg(SUCCESS_COLOR),
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
    fn render_system_entry_splits_title_and_detail_lines() {
        let entry = ChatEntry::new(
            ChatRole::System,
            "Context compacted · auto · -4 msgs\nsummary · older turns".to_string(),
        );
        let mut lines = Vec::new();
        render_system_entry(&mut lines, &entry);
        assert_eq!(lines.len(), 4);
        assert!(lines[0].to_string().contains("Context compacted"));
        assert!(lines[1].to_string().contains("auto · -4 msgs"));
        assert!(lines[2].to_string().contains("summary · older turns"));
        assert!(lines[3].to_string().contains("ctrl+o to inspect"));
    }

    #[test]
    fn render_system_entry_hyperlinks_detail_urls() {
        let entry = ChatEntry::new(
            ChatRole::System,
            "Session memory updated · ref · https://example.com/docs".to_string(),
        );
        let mut lines = Vec::new();
        render_system_entry(&mut lines, &entry);
        assert!(lines.iter().any(|line| line
            .to_string()
            .contains("\u{1b}]8;;https://example.com/docs")));
    }

    #[test]
    fn render_grouped_system_entries_renders_batch_title() {
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
        let mut lines = Vec::new();
        render_grouped_system_entries(&mut lines, &entries, &batch);
        assert!(lines[0].to_string().contains("Status updates(2)"));
        assert!(lines[0].to_string().contains("ctrl+o to inspect"));
        assert!(lines[1].to_string().contains("Context compacted"));
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
        assert!(lines[0].to_string().contains("ctrl+o to inspect"));
    }

    #[test]
    fn render_grouped_system_entries_uses_hook_batch_title() {
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
        let mut lines = Vec::new();
        render_grouped_system_entries(&mut lines, &entries, &batch);
        assert!(lines[0].to_string().contains("Hook updates(2)"));
    }

    #[test]
    fn render_grouped_system_entries_prefers_latest_items_when_trimming() {
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
        let mut lines = Vec::new();
        render_grouped_system_entries(&mut lines, &entries, &batch);
        let rendered = lines
            .iter()
            .map(|line| line.to_string())
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
    fn render_turn_summary_entry_uses_turn_styling() {
        let entry = ChatEntry::new(
            ChatRole::System,
            "Turn completed · 1.4s · 3 tools · 1.2k↑ 180↓ tok\nsession · 15.4k total tok · 34 tools"
                .to_string(),
        );
        let mut lines = Vec::new();
        render_system_entry(&mut lines, &entry);
        assert!(lines[0].to_string().contains("Turn completed"));
        assert!(lines[1].to_string().contains("1.4s · 3 tools"));
        assert!(lines[2].to_string().contains("session · 15.4k total tok"));
    }

    #[test]
    fn grouped_system_entries_names_remote_review_and_workflow_batches() {
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
            let mut lines = Vec::new();
            render_grouped_system_entries(&mut lines, &entries, &batch);
            assert!(lines[0].to_string().contains(expected));
        }
    }
}
