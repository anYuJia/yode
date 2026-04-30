mod style;
mod subagents;
mod system;
mod tool_calls;
mod users;

use ratatui::style::{Color, Modifier};

use crate::app::{ChatEntry, ChatRole};
use crate::tool_grouping::{should_hide_tool_from_transcript, SubAgentBatch};
use crate::ui::error_format::parse_error_view;

use self::style::role_style_palette;
use self::subagents::{render_grouped_subagent_batch, render_subagent_call};
use self::system::{render_grouped_system_entries, render_system_entry};
use self::tool_calls::{render_grouped_tool_call, render_standalone_result, render_tool_call};
use self::users::{render_assistant, render_user};

pub(crate) fn format_entry_as_strings(
    entry: &ChatEntry,
    all_entries: &[ChatEntry],
    index: usize,
) -> Vec<(String, ratatui::style::Style)> {
    let mut result: Vec<(String, ratatui::style::Style)> = Vec::new();
    let palette = role_style_palette();
    let latest_reasoning_index = all_entries
        .iter()
        .enumerate()
        .rev()
        .find(|(_, item)| {
            matches!(item.role, ChatRole::Assistant)
                && item
                    .reasoning
                    .as_deref()
                    .is_some_and(|reasoning| !reasoning.trim().is_empty())
        })
        .map(|(idx, _)| idx);
    let latest_system_index = all_entries
        .iter()
        .enumerate()
        .rev()
        .find(|(_, item)| matches!(item.role, ChatRole::System))
        .map(|(idx, _)| idx);
    let latest_error_index = all_entries
        .iter()
        .enumerate()
        .rev()
        .find(|(_, item)| matches!(item.role, ChatRole::Error))
        .map(|(idx, _)| idx);
    let latest_tool_index = all_entries
        .iter()
        .enumerate()
        .rev()
        .find(|(idx, item)| match &item.role {
            ChatRole::ToolCall { .. } => true,
            ChatRole::ToolResult { id, .. } => !all_entries[..*idx].iter().rev().any(
                |entry| matches!(&entry.role, ChatRole::ToolCall { id: tid, .. } if tid == id),
            ),
            _ => false,
        })
        .map(|(idx, _)| idx);

    match &entry.role {
        ChatRole::User => render_user(entry, &mut result, palette.cyan),
        ChatRole::Assistant => render_assistant(
            entry,
            &mut result,
            palette.dim,
            palette.white,
            latest_reasoning_index == Some(index),
        ),
        ChatRole::ToolCall { id: tid, name } => {
            if should_hide_tool_from_transcript(name) {
                return result;
            }
            render_tool_call(
                entry,
                all_entries,
                index,
                tid,
                name,
                &mut result,
                palette.dim,
                palette.accent,
                palette.red,
                latest_tool_index == Some(index),
            )
        }
        ChatRole::ToolResult { id: rid, .. } => {
            if let ChatRole::ToolResult { name, .. } = &entry.role {
                if should_hide_tool_from_transcript(name) {
                    return result;
                }
            }
            let has_preceding = index > 0
                && all_entries[..index].iter().rev().any(
                    |e| matches!(&e.role, ChatRole::ToolCall { id: ref tid, .. } if tid == rid),
                );
            if !has_preceding {
                render_standalone_result(
                    entry,
                    &mut result,
                    palette.dim,
                    palette.accent,
                    palette.red,
                    latest_tool_index == Some(index),
                );
            }
        }
        ChatRole::Error => {
            let view = parse_error_view(&entry.content);
            result.push((
                format!("  ! {}", view.title),
                ratatui::style::Style::default()
                    .fg(Color::LightRed)
                    .add_modifier(Modifier::BOLD),
            ));
            if latest_error_index == Some(index) {
                if let Some(first_line) = view.detail_lines.first() {
                    result.push((
                        format!("    {}", first_line),
                        ratatui::style::Style::default().fg(Color::Yellow),
                    ));
                }
                if view.detail_lines.len() > 1 {
                    result.push((
                        format!(
                            "    … +{} more lines (ctrl+o to inspect)",
                            view.detail_lines.len() - 1
                        ),
                        ratatui::style::Style::default().fg(Color::Gray),
                    ));
                }
            }
            result.push((
                "    ctrl+o to inspect".to_string(),
                ratatui::style::Style::default()
                    .fg(Color::Gray)
                    .add_modifier(Modifier::ITALIC),
            ));
        }
        ChatRole::System => {
            result.extend(render_system_entry(
                entry,
                latest_system_index == Some(index),
            ));
        }
        ChatRole::SubAgentCall { description } => {
            render_subagent_call(
                description,
                all_entries,
                index,
                &mut result,
                palette.dim,
                palette.accent,
            );
        }
        ChatRole::SubAgentToolCall { .. } => {}
        ChatRole::SubAgentResult => {}
        ChatRole::AskUser { .. } => {
            for (line_index, line) in entry.content.lines().enumerate() {
                result.push((
                    format!("{}{}", if line_index == 0 { "  ? " } else { "    " }, line),
                    palette.accent,
                ));
            }
        }
    }
    result
}

pub(crate) fn format_grouped_tool_batch(
    all_entries: &[ChatEntry],
    batch: &crate::tool_grouping::ToolBatch,
) -> Vec<(String, ratatui::style::Style)> {
    let mut result: Vec<(String, ratatui::style::Style)> = Vec::new();
    let palette = role_style_palette();
    render_grouped_tool_call(all_entries, batch, &mut result, palette.dim, palette.accent);
    result
}

pub(crate) fn format_grouped_system_batch(
    all_entries: &[ChatEntry],
    batch: &crate::tool_grouping::SystemBatch,
) -> Vec<(String, ratatui::style::Style)> {
    render_grouped_system_entries(all_entries, batch)
}

pub(crate) fn format_grouped_subagent_batch(
    all_entries: &[ChatEntry],
    batch: &SubAgentBatch,
) -> Vec<(String, ratatui::style::Style)> {
    let mut result: Vec<(String, ratatui::style::Style)> = Vec::new();
    let palette = role_style_palette();
    render_grouped_subagent_batch(all_entries, batch, &mut result, palette.dim, palette.accent);
    result
}

#[cfg(test)]
mod tests {
    use crate::app::{ChatEntry, ChatRole};

    use super::{format_entry_as_strings, format_grouped_system_batch};
    use crate::tool_grouping::{SystemBatch, SystemBatchItem};

    #[test]
    fn error_entries_include_inspector_hint() {
        let entry = ChatEntry::new(
            ChatRole::Error,
            "something odd happened\nwith more detail".to_string(),
        );
        let rendered = format_entry_as_strings(&entry, std::slice::from_ref(&entry), 0);
        assert!(rendered.iter().any(|(line, _)| line.contains("! Error")));
        assert!(rendered
            .iter()
            .any(|(line, _)| line.contains("something odd happened")));
        assert!(rendered
            .iter()
            .any(|(line, _)| line.contains("+1 more lines")));
        assert!(rendered
            .iter()
            .any(|(line, _)| line.contains("ctrl+o to inspect")));
        assert!(rendered.iter().all(|(line, _)| !line.contains("╭─ Error")));
    }

    #[test]
    fn scrollback_formats_keep_inspector_discoverability_for_core_roles() {
        let tool_entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "read_file".to_string(),
                },
                "{\"file_path\":\"/tmp/src/main.rs\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "a".to_string(),
                    name: "read_file".to_string(),
                    is_error: false,
                },
                "fn main() {}".to_string(),
            ),
        ];
        let tool_rendered = format_entry_as_strings(&tool_entries[0], &tool_entries, 0);
        assert!(tool_rendered
            .iter()
            .any(|(line, _)| line.contains("ctrl+o to inspect")));

        let system_entries = vec![
            ChatEntry::new(
                ChatRole::System,
                "Context compacted · auto · -4 msgs".to_string(),
            ),
            ChatEntry::new(
                ChatRole::System,
                "Session memory updated · summary · /tmp/live.md".to_string(),
            ),
        ];
        let system_rendered = format_grouped_system_batch(
            &system_entries,
            &SystemBatch {
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
            },
        );
        assert!(system_rendered
            .iter()
            .any(|(line, _)| line.contains("ctrl+o to inspect")));

        let error_entry = ChatEntry::new(
            ChatRole::Error,
            "OpenAI API error (400): This model's maximum context length is 128000 tokens."
                .to_string(),
        );
        let error_rendered =
            format_entry_as_strings(&error_entry, std::slice::from_ref(&error_entry), 0);
        assert!(error_rendered
            .iter()
            .any(|(line, _)| line.contains("ctrl+o to inspect")));
    }

    #[test]
    fn latest_system_entry_keeps_detail_while_older_entries_collapse() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::System,
                "Session memory updated · summary · /tmp/a.md\nnote · older".to_string(),
            ),
            ChatEntry::new(
                ChatRole::System,
                "Session memory updated · summary · /tmp/b.md\nnote · latest".to_string(),
            ),
        ];
        let older = format_entry_as_strings(&entries[0], &entries, 0);
        let latest = format_entry_as_strings(&entries[1], &entries, 1);
        assert!(older.iter().all(|(line, _)| !line.contains("/tmp/a.md")));
        assert!(latest.iter().any(|(line, _)| line.contains("/tmp/b.md")));
    }

    #[test]
    fn latest_error_entry_keeps_detail_while_older_entries_collapse() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::Error,
                "OpenAI API error (400): This model's maximum context length is 128000 tokens."
                    .to_string(),
            ),
            ChatEntry::new(
                ChatRole::Error,
                "OpenAI API error (400): This model's maximum context length is 128000 tokens."
                    .to_string(),
            ),
        ];
        let older = format_entry_as_strings(&entries[0], &entries, 0);
        let latest = format_entry_as_strings(&entries[1], &entries, 1);
        assert!(older
            .iter()
            .all(|(line, _)| !line.contains("The request exceeded the model context window.")));
        assert!(latest
            .iter()
            .any(|(line, _)| line.contains("The request exceeded the model context window.")));
    }

    #[test]
    fn latest_tool_entry_keeps_metadata_while_older_entries_collapse() {
        let mut first_result = ChatEntry::new(
            ChatRole::ToolResult {
                id: "a".to_string(),
                name: "powershell".to_string(),
                is_error: false,
            },
            "ok".to_string(),
        );
        first_result.tool_metadata = Some(serde_json::json!({
            "read_only_reason": "older metadata"
        }));
        let mut second_result = ChatEntry::new(
            ChatRole::ToolResult {
                id: "b".to_string(),
                name: "powershell".to_string(),
                is_error: false,
            },
            "ok".to_string(),
        );
        second_result.tool_metadata = Some(serde_json::json!({
            "read_only_reason": "latest metadata"
        }));
        let entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "powershell".to_string(),
                },
                "{\"command\":\"Get-Content a.txt\"}".to_string(),
            ),
            first_result,
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "b".to_string(),
                    name: "powershell".to_string(),
                },
                "{\"command\":\"Get-Content b.txt\"}".to_string(),
            ),
            second_result,
        ];
        let older = format_entry_as_strings(&entries[0], &entries, 0);
        let latest = format_entry_as_strings(&entries[2], &entries, 2);
        assert!(older
            .iter()
            .all(|(line, _)| !line.contains("older metadata")));
        assert!(latest
            .iter()
            .any(|(line, _)| line.contains("latest metadata")));
    }

    #[test]
    fn latest_focus_mixed_tool_system_and_error_runs() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "read_file".to_string(),
                },
                "{\"file_path\":\"/tmp/old.rs\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "a".to_string(),
                    name: "read_file".to_string(),
                    is_error: false,
                },
                "old".to_string(),
            ),
            ChatEntry::new(
                ChatRole::System,
                "Session memory updated · summary · /tmp/older.md\nolder detail".to_string(),
            ),
            ChatEntry::new(ChatRole::Error, "older error\nhidden detail".to_string()),
            ChatEntry::new(
                ChatRole::System,
                "Session memory updated · summary · /tmp/latest.md\nlatest detail".to_string(),
            ),
            ChatEntry::new(ChatRole::Error, "latest error\nvisible detail".to_string()),
        ];
        let older_system = format_entry_as_strings(&entries[2], &entries, 2);
        let latest_system = format_entry_as_strings(&entries[4], &entries, 4);
        let older_error = format_entry_as_strings(&entries[3], &entries, 3);
        let latest_error = format_entry_as_strings(&entries[5], &entries, 5);

        assert!(older_system
            .iter()
            .all(|(line, _)| !line.contains("older detail")));
        assert!(latest_system
            .iter()
            .any(|(line, _)| line.contains("/tmp/latest.md")));
        assert!(latest_system
            .iter()
            .any(|(line, _)| line.contains("+1 more lines")));
        assert!(older_error
            .iter()
            .all(|(line, _)| !line.contains("hidden detail")));
        assert!(latest_error
            .iter()
            .any(|(line, _)| line.contains("latest error")));
    }

    #[test]
    fn ask_user_entries_render_question_framing() {
        let entry = ChatEntry::new(
            ChatRole::AskUser {
                id: "ask-1".to_string(),
            },
            "Choose a deployment target\nstaging or prod".to_string(),
        );
        let rendered = format_entry_as_strings(&entry, std::slice::from_ref(&entry), 0);
        assert!(rendered[0].0.starts_with("  ? Choose"));
        assert!(rendered[1].0.starts_with("    staging"));
    }

    #[test]
    fn transcript_line_prefixes_stay_consistent_across_core_roles() {
        let assistant = ChatEntry::new(ChatRole::Assistant, "Final answer".to_string());
        let assistant_lines =
            format_entry_as_strings(&assistant, std::slice::from_ref(&assistant), 0);
        assert!(assistant_lines[1].0.starts_with("⏺ "));

        let tool_entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "read_file".to_string(),
                },
                "{\"file_path\":\"/tmp/src/main.rs\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "a".to_string(),
                    name: "read_file".to_string(),
                    is_error: false,
                },
                "fn main() {}".to_string(),
            ),
        ];
        let tool_lines = format_entry_as_strings(&tool_entries[0], &tool_entries, 0);
        assert!(tool_lines[0].0.starts_with("⏺ "));

        let system = ChatEntry::new(
            ChatRole::System,
            "Session memory updated · summary · /tmp/live.md".to_string(),
        );
        let system_lines = format_entry_as_strings(&system, std::slice::from_ref(&system), 0);
        assert!(system_lines[0].0.starts_with("  ≈ "));

        let error = ChatEntry::new(
            ChatRole::Error,
            "OpenAI API error (400): This model's maximum context length is 128000 tokens."
                .to_string(),
        );
        let error_lines = format_entry_as_strings(&error, std::slice::from_ref(&error), 0);
        assert!(error_lines[0].0.starts_with("  ! "));
    }
}
