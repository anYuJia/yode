use crate::app::{ChatEntry, ChatRole};
use crate::system_message::{parse_system_message, SystemMessageKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolBatchItem {
    pub call_index: usize,
    pub result_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolBatch {
    pub tool_name: String,
    pub start_index: usize,
    pub next_index: usize,
    pub items: Vec<ToolBatchItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SystemBatchItem {
    pub entry_index: usize,
    pub kind: SystemMessageKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SystemBatch {
    pub start_index: usize,
    pub next_index: usize,
    pub items: Vec<SystemBatchItem>,
}

pub(crate) fn detect_groupable_tool_batch(
    entries: &[ChatEntry],
    start_index: usize,
) -> Option<ToolBatch> {
    let ChatRole::ToolCall { name, .. } = &entries.get(start_index)?.role else {
        return None;
    };
    if !is_groupable_tool(name) {
        return None;
    }

    let tool_name = name.clone();
    let mut index = start_index;
    let mut items = Vec::new();

    while index < entries.len() {
        let Some(entry) = entries.get(index) else {
            break;
        };
        let ChatRole::ToolCall { id, name } = &entry.role else {
            break;
        };
        if name != &tool_name {
            break;
        }

        let result_index = entries.get(index + 1).and_then(|next| match &next.role {
            ChatRole::ToolResult { id: result_id, .. } if result_id == id => Some(index + 1),
            _ => None,
        });

        items.push(ToolBatchItem {
            call_index: index,
            result_index,
        });
        index = result_index.map(|value| value + 1).unwrap_or(index + 1);
    }

    if items.len() < 2 {
        return None;
    }

    Some(ToolBatch {
        tool_name,
        start_index,
        next_index: index,
        items,
    })
}

pub(crate) fn detect_groupable_system_batch(
    entries: &[ChatEntry],
    start_index: usize,
) -> Option<SystemBatch> {
    let entry = entries.get(start_index)?;
    if !matches!(entry.role, ChatRole::System) {
        return None;
    }
    let first_kind = parse_system_message(&entry.content).kind;
    if !is_groupable_system_kind(first_kind) {
        return None;
    }

    let mut index = start_index;
    let mut items = Vec::new();
    while index < entries.len() {
        let Some(entry) = entries.get(index) else {
            break;
        };
        if !matches!(entry.role, ChatRole::System) {
            break;
        }
        let kind = parse_system_message(&entry.content).kind;
        if !is_groupable_system_kind(kind) {
            break;
        }
        items.push(SystemBatchItem {
            entry_index: index,
            kind,
        });
        index += 1;
    }

    if items.len() < 2 {
        return None;
    }

    Some(SystemBatch {
        start_index,
        next_index: index,
        items,
    })
}

fn is_groupable_tool(name: &str) -> bool {
    matches!(name, "read_file" | "grep" | "glob")
}

fn is_groupable_system_kind(kind: SystemMessageKind) -> bool {
    matches!(
        kind,
        SystemMessageKind::Context
            | SystemMessageKind::Memory
            | SystemMessageKind::Budget
            | SystemMessageKind::Export
            | SystemMessageKind::Task
            | SystemMessageKind::Update
    )
}

#[cfg(test)]
mod tests {
    use crate::app::{ChatEntry, ChatRole};

    use super::{detect_groupable_system_batch, detect_groupable_tool_batch};

    #[test]
    fn detects_consecutive_groupable_tool_calls() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "read_file".to_string(),
                },
                "{\"file_path\":\"/tmp/a\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "a".to_string(),
                    name: "read_file".to_string(),
                    is_error: false,
                },
                "ok".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "b".to_string(),
                    name: "read_file".to_string(),
                },
                "{\"file_path\":\"/tmp/b\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "b".to_string(),
                    name: "read_file".to_string(),
                    is_error: false,
                },
                "ok".to_string(),
            ),
        ];

        let batch = detect_groupable_tool_batch(&entries, 0).unwrap();
        assert_eq!(batch.tool_name, "read_file");
        assert_eq!(batch.items.len(), 2);
        assert_eq!(batch.next_index, 4);
    }

    #[test]
    fn detects_consecutive_groupable_system_messages() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::System,
                "Context compressed · auto · -4 msgs".to_string(),
            ),
            ChatEntry::new(
                ChatRole::System,
                "Session memory updated · summary · /tmp/live.md".to_string(),
            ),
            ChatEntry::new(ChatRole::Assistant, "ok".to_string()),
        ];

        let batch = detect_groupable_system_batch(&entries, 0).unwrap();
        assert_eq!(batch.items.len(), 2);
        assert_eq!(batch.next_index, 2);
    }
}
