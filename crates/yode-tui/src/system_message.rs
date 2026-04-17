#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SystemMessageKind {
    Context,
    Memory,
    Budget,
    Export,
    Task,
    Warning,
    Lifecycle,
    Plan,
    Update,
    Generic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SystemMessageView {
    pub kind: SystemMessageKind,
    pub title: String,
    pub detail_lines: Vec<String>,
}

pub(crate) fn parse_system_message(content: &str) -> SystemMessageView {
    let lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    let Some(first_line) = lines.first() else {
        return SystemMessageView {
            kind: SystemMessageKind::Generic,
            title: String::new(),
            detail_lines: Vec::new(),
        };
    };

    let (kind, title, first_detail) =
        if let Some(detail) = strip_title_suffix(first_line, "Context compressed") {
            (
                SystemMessageKind::Context,
                "Context compressed".to_string(),
                detail,
            )
        } else if let Some(detail) = strip_title_suffix(first_line, "Session memory updated") {
            (
                SystemMessageKind::Memory,
                "Session memory updated".to_string(),
                detail,
            )
        } else if let Some(detail) = strip_title_suffix(first_line, "Budget exceeded") {
            (
                SystemMessageKind::Budget,
                "Budget exceeded".to_string(),
                detail,
            )
        } else if let Some((title, detail)) = split_task_line(first_line) {
            (SystemMessageKind::Task, title, detail)
        } else if let Some(detail) = strip_title_suffix(first_line, "Background tasks still running") {
            (
                SystemMessageKind::Task,
                "Background tasks still running".to_string(),
                detail,
            )
        } else if let Some((title, detail)) = split_export_line(first_line) {
            (SystemMessageKind::Export, title, detail)
        } else if let Some((title, detail)) = split_warning_line(first_line) {
            (SystemMessageKind::Warning, title, detail)
        } else if is_lifecycle_message(first_line) {
            (SystemMessageKind::Lifecycle, first_line.clone(), None)
        } else if is_plan_message(first_line) {
            (SystemMessageKind::Plan, first_line.clone(), None)
        } else if is_update_message(first_line) {
            (SystemMessageKind::Update, first_line.clone(), None)
        } else {
            (SystemMessageKind::Generic, first_line.clone(), None)
        };

    let mut detail_lines = Vec::new();
    if let Some(detail) = first_detail.filter(|detail| !detail.is_empty()) {
        detail_lines.push(detail);
    }
    detail_lines.extend(lines.into_iter().skip(1));

    SystemMessageView {
        kind,
        title,
        detail_lines,
    }
}

pub(crate) fn system_message_summary(view: &SystemMessageView) -> String {
    let mut summary = view.title.clone();
    if let Some(first_detail) = view.detail_lines.first() {
        summary.push_str(" · ");
        summary.push_str(first_detail);
    }
    if view.detail_lines.len() > 1 {
        summary.push_str(&format!(" · +{} more", view.detail_lines.len() - 1));
    }
    summary
}

pub(crate) fn append_grouped_system_entry(
    entries: &mut Vec<ChatEntry>,
    content: impl Into<String>,
) {
    let content = content.into();
    if let Some(last) = entries.last_mut() {
        if matches!(last.role, ChatRole::System) && last.timestamp.elapsed() <= Duration::from_secs(5)
        {
            let last_view = parse_system_message(&last.content);
            let next_view = parse_system_message(&content);
            let same_semantic_group = next_view.kind != SystemMessageKind::Generic
                && last_view.kind == next_view.kind
                && last_view.title == next_view.title;
            let same_first_line = last.content.lines().next() == content.lines().next();
            if same_semantic_group || same_first_line {
                if !last.content.contains(&content) {
                    last.content.push('\n');
                    last.content.push_str(&content);
                }
                return;
            }
        }
    }
    entries.push(ChatEntry::new(ChatRole::System, content));
}

fn strip_title_suffix(line: &str, title: &str) -> Option<Option<String>> {
    let suffix = line.strip_prefix(title)?;
    let detail = suffix
        .trim_start_matches([' ', '·', ':'])
        .trim()
        .to_string();
    if detail.is_empty() {
        Some(None)
    } else {
        Some(Some(detail))
    }
}

fn split_export_line(line: &str) -> Option<(String, Option<String>)> {
    let exported_marker = " exported to:";
    let (title, detail) = line.split_once(exported_marker)?;
    let detail = detail.trim().to_string();
    Some((
        format!("{} exported", title.trim()),
        if detail.is_empty() {
            None
        } else {
            Some(detail)
        },
    ))
}

fn split_task_line(line: &str) -> Option<(String, Option<String>)> {
    let severity = line.strip_prefix("[Task:")?.split_once(']')?;
    let title = format!("Task {}", severity.0.trim());
    let detail = severity.1.trim().to_string();
    Some((
        title,
        if detail.is_empty() {
            None
        } else {
            Some(detail)
        },
    ))
}

fn split_warning_line(line: &str) -> Option<(String, Option<String>)> {
    if let Some(detail) = strip_title_suffix(line, "⚠ Dangerous command detected") {
        return Some(("Dangerous command detected".to_string(), detail));
    }
    if let Some(detail) = line.strip_prefix("Unknown command:") {
        let detail = detail.trim().to_string();
        return Some((
            "Unknown command".to_string(),
            if detail.is_empty() {
                None
            } else {
                Some(detail)
            },
        ));
    }
    None
}

fn is_lifecycle_message(line: &str) -> bool {
    matches!(
        line,
        "Session resumed." | "Generation cancelled." | "Wizard cancelled."
    )
}

fn is_plan_message(line: &str) -> bool {
    line.starts_with("[Plan mode]")
        || line.contains("Entered plan mode")
        || line.contains("Plan ready for approval")
        || line.contains("Exited plan mode")
}

fn is_update_message(line: &str) -> bool {
    line.contains("Update available")
        || line.contains("Downloading update")
        || line.contains("ready (restart to apply)")
}

#[cfg(test)]
mod tests {
    use super::{
        append_grouped_system_entry, parse_system_message, system_message_summary,
        SystemMessageKind,
    };
    use crate::app::{ChatEntry, ChatRole};

    #[test]
    fn parses_compaction_messages_into_title_and_details() {
        let view = parse_system_message(
            "Context compressed · auto · -4 msgs\nsummary · trimmed older turns",
        );
        assert_eq!(view.kind, SystemMessageKind::Context);
        assert_eq!(view.title, "Context compressed");
        assert_eq!(view.detail_lines[0], "auto · -4 msgs");
        assert_eq!(view.detail_lines[1], "summary · trimmed older turns");
    }

    #[test]
    fn parses_export_messages_into_title_and_path() {
        let view = parse_system_message("Diagnostics bundle exported to: /tmp/bundle");
        assert_eq!(view.kind, SystemMessageKind::Export);
        assert_eq!(view.title, "Diagnostics bundle exported");
        assert_eq!(view.detail_lines, vec!["/tmp/bundle".to_string()]);
    }

    #[test]
    fn leaves_generic_messages_as_title_only() {
        let view = parse_system_message("Generation cancelled.");
        assert_eq!(view.kind, SystemMessageKind::Lifecycle);
        assert_eq!(view.title, "Generation cancelled.");
        assert!(view.detail_lines.is_empty());
    }

    #[test]
    fn summary_compacts_extra_detail_lines() {
        let view = parse_system_message(
            "Context compressed · auto · -4 msgs\nsummary · trimmed older turns",
        );
        assert_eq!(
            system_message_summary(&view),
            "Context compressed · auto · -4 msgs · +1 more"
        );
    }

    #[test]
    fn parses_task_notifications() {
        let view = parse_system_message("[Task:warn] agent stalled");
        assert_eq!(view.kind, SystemMessageKind::Task);
        assert_eq!(view.title, "Task warn");
        assert_eq!(view.detail_lines, vec!["agent stalled".to_string()]);
    }

    #[test]
    fn parses_warning_messages() {
        let view = parse_system_message("Unknown command: /oops. Type /help.");
        assert_eq!(view.kind, SystemMessageKind::Warning);
        assert_eq!(view.title, "Unknown command");
    }

    #[test]
    fn append_grouped_system_entry_merges_semantic_duplicates() {
        let mut entries = vec![ChatEntry::new(
            ChatRole::System,
            "Session memory updated · summary · /tmp/a.md".to_string(),
        )];
        append_grouped_system_entry(
            &mut entries,
            "Session memory updated · snapshot · /tmp/b.md".to_string(),
        );
        assert_eq!(entries.len(), 1);
        assert!(entries[0].content.contains("/tmp/a.md"));
        assert!(entries[0].content.contains("/tmp/b.md"));
    }
}
use std::time::Duration;

use crate::app::{ChatEntry, ChatRole};
