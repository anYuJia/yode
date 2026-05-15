use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use yode_llm::types::{Message, Role};

const COLLAPSE_PREFIX: &str = "[Context collapse]";
const COLLAPSE_MIN_TOOL_CHARS: usize = 2_000;
const COLLAPSE_PREVIEW_CHARS: usize = 240;
const COLLAPSE_PRESERVE_RECENT: usize = 8;
const COLLAPSE_MAX_PER_OPERATION: usize = 3;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContextCollapseOperation {
    pub id: String,
    pub created_at: String,
    pub source_ranges: Vec<ContextCollapseSourceRange>,
    pub replacements: Vec<ContextCollapseReplacement>,
    pub saved_chars: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContextCollapseSourceRange {
    pub message_index: usize,
    pub role: String,
    pub tool_call_id: Option<String>,
    pub original_chars: usize,
    pub collapsed_chars: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContextCollapseReplacement {
    pub message_index: usize,
    pub original_content: String,
    pub collapsed_content: String,
}

pub fn is_context_collapse_enabled() -> bool {
    std::env::var("YODE_CONTEXT_COLLAPSE")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "on"))
        .unwrap_or(false)
}

pub fn collapse_tool_heavy_spans(messages: &mut [Message]) -> Option<ContextCollapseOperation> {
    if messages.len() <= COLLAPSE_PRESERVE_RECENT + 1 {
        return None;
    }

    let collapse_end = messages.len().saturating_sub(COLLAPSE_PRESERVE_RECENT);
    let mut operation = ContextCollapseOperation {
        id: uuid::Uuid::new_v4().to_string(),
        created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        source_ranges: Vec::new(),
        replacements: Vec::new(),
        saved_chars: 0,
    };

    for (index, message) in messages.iter_mut().enumerate().take(collapse_end).skip(1) {
        if operation.replacements.len() >= COLLAPSE_MAX_PER_OPERATION {
            break;
        }
        if !matches!(message.role, Role::Tool) {
            continue;
        }
        let Some(original) = message.content.as_ref() else {
            continue;
        };
        if original.starts_with(COLLAPSE_PREFIX)
            || original.chars().count() < COLLAPSE_MIN_TOOL_CHARS
        {
            continue;
        }

        let original_content = original.clone();
        let collapsed_content = summarize_tool_output(&original_content);
        if collapsed_content == original_content {
            continue;
        }
        let original_chars = original_content.chars().count();
        let collapsed_chars = collapsed_content.chars().count();
        operation.saved_chars = operation
            .saved_chars
            .saturating_add(original_chars.saturating_sub(collapsed_chars));
        operation.source_ranges.push(ContextCollapseSourceRange {
            message_index: index,
            role: "tool".to_string(),
            tool_call_id: message.tool_call_id.clone(),
            original_chars,
            collapsed_chars,
        });
        operation.replacements.push(ContextCollapseReplacement {
            message_index: index,
            original_content,
            collapsed_content: collapsed_content.clone(),
        });
        message.content = Some(collapsed_content);
        message.normalize_in_place();
    }

    (!operation.replacements.is_empty()).then_some(operation)
}

pub fn replay_context_collapse(
    messages: &mut [Message],
    operation: &ContextCollapseOperation,
) -> bool {
    let mut changed = false;
    for replacement in &operation.replacements {
        let Some(message) = messages.get_mut(replacement.message_index) else {
            continue;
        };
        if message.content.as_deref() == Some(replacement.original_content.as_str()) {
            message.content = Some(replacement.collapsed_content.clone());
            message.normalize_in_place();
            changed = true;
        }
    }
    changed
}

pub fn reverse_context_collapse(
    messages: &mut [Message],
    operation: &ContextCollapseOperation,
) -> bool {
    let mut changed = false;
    for replacement in &operation.replacements {
        let Some(message) = messages.get_mut(replacement.message_index) else {
            continue;
        };
        if message.content.as_deref() == Some(replacement.collapsed_content.as_str()) {
            message.content = Some(replacement.original_content.clone());
            message.normalize_in_place();
            changed = true;
        }
    }
    changed
}

pub fn write_context_collapse_artifact(
    project_root: &Path,
    session_id: &str,
    operation: &ContextCollapseOperation,
) -> Result<PathBuf> {
    let dir = project_root.join(".yode").join("context-collapse");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create context collapse dir: {}", dir.display()))?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!(
        "{}-collapse-{}.json",
        short_session,
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    ));
    std::fs::write(&path, serde_json::to_string_pretty(operation)?).with_context(|| {
        format!(
            "Failed to write context collapse artifact: {}",
            path.display()
        )
    })?;
    Ok(path)
}

fn summarize_tool_output(content: &str) -> String {
    let preview = content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(COLLAPSE_PREVIEW_CHARS)
        .collect::<String>();
    format!(
        "{} Tool output collapsed to reduce context pressure. Original length: {} chars. Preview: {}",
        COLLAPSE_PREFIX,
        content.chars().count(),
        preview
    )
}

#[cfg(test)]
mod tests {
    use super::{
        collapse_tool_heavy_spans, replay_context_collapse, reverse_context_collapse,
        write_context_collapse_artifact,
    };
    use tempfile::tempdir;
    use yode_llm::types::Message;

    #[test]
    fn collapse_operation_replays_and_reverses_tool_output() {
        let big = "tool output ".repeat(400);
        let mut messages = vec![
            Message::system("system"),
            Message::tool_result("tool-1", &big),
            Message::user("recent 1"),
            Message::assistant("recent 2"),
            Message::user("recent 3"),
            Message::assistant("recent 4"),
            Message::user("recent 5"),
            Message::assistant("recent 6"),
            Message::user("recent 7"),
            Message::assistant("recent 8"),
            Message::user("recent 9"),
        ];
        let operation = collapse_tool_heavy_spans(&mut messages).expect("collapse operation");
        assert_eq!(operation.replacements.len(), 1);
        assert!(operation.saved_chars > 0);
        assert!(messages[1]
            .content
            .as_deref()
            .unwrap()
            .starts_with("[Context collapse]"));

        let mut replay_messages = vec![
            Message::system("system"),
            Message::tool_result("tool-1", &big),
            Message::user("recent 1"),
            Message::assistant("recent 2"),
            Message::user("recent 3"),
            Message::assistant("recent 4"),
            Message::user("recent 5"),
            Message::assistant("recent 6"),
            Message::user("recent 7"),
            Message::assistant("recent 8"),
            Message::user("recent 9"),
        ];
        assert!(replay_context_collapse(&mut replay_messages, &operation));
        assert_eq!(replay_messages[1].content, messages[1].content);
        assert!(reverse_context_collapse(&mut replay_messages, &operation));
        assert_eq!(replay_messages[1].content.as_deref(), Some(big.as_str()));
    }

    #[test]
    fn collapse_artifact_contains_source_ranges() {
        let temp = tempdir().unwrap();
        let big = "artifact output ".repeat(400);
        let mut messages = vec![
            Message::system("system"),
            Message::tool_result("tool-1", &big),
            Message::user("recent 1"),
            Message::assistant("recent 2"),
            Message::user("recent 3"),
            Message::assistant("recent 4"),
            Message::user("recent 5"),
            Message::assistant("recent 6"),
            Message::user("recent 7"),
            Message::assistant("recent 8"),
            Message::user("recent 9"),
        ];
        let operation = collapse_tool_heavy_spans(&mut messages).unwrap();
        let path =
            write_context_collapse_artifact(temp.path(), "session-abcdef", &operation).unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("\"source_ranges\""));
        assert!(content.contains("\"replacements\""));
    }
}
