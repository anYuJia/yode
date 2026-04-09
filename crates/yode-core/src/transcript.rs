use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use yode_llm::types::{Message, Role};

use crate::context_manager::CompressionReport;

const TRANSCRIPTS_DIR: &str = ".yode/transcripts";

pub fn write_compaction_transcript(
    project_root: &Path,
    session_id: &str,
    messages: &[Message],
    report: &CompressionReport,
    mode: &str,
    failed_tool_call_ids: &HashSet<String>,
) -> Result<PathBuf> {
    let dir = project_root.join(TRANSCRIPTS_DIR);
    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create transcript dir: {}", dir.display()))?;

    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!(
        "{}-compact-{}.md",
        short_session_id(session_id),
        timestamp
    ));
    fs::write(
        &path,
        render_compaction_transcript(session_id, messages, report, mode, failed_tool_call_ids),
    )
    .with_context(|| format!("Failed to write transcript file: {}", path.display()))?;

    Ok(path)
}

fn render_compaction_transcript(
    session_id: &str,
    messages: &[Message],
    report: &CompressionReport,
    mode: &str,
    failed_tool_call_ids: &HashSet<String>,
) -> String {
    let failure_summary = summarize_failed_tools(messages, failed_tool_call_ids);
    let mut output = String::new();
    output.push_str("# Compaction Transcript\n\n");
    output.push_str(&format!("- Session: {}\n", session_id));
    output.push_str(&format!("- Mode: {}\n", mode));
    output.push_str(&format!(
        "- Timestamp: {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    output.push_str(&format!("- Removed messages: {}\n", report.removed));
    output.push_str(&format!(
        "- Tool results truncated: {}\n",
        report.tool_results_truncated
    ));
    output.push_str(&format!(
        "- Failed tool results: {}\n",
        failure_summary.failed_tool_results
    ));
    if !failure_summary.failed_tool_names.is_empty() {
        output.push_str(&format!(
            "- Failed tools: {}\n",
            failure_summary.failed_tool_names.join(", ")
        ));
    }
    if let Some(summary) = report.summary.as_deref() {
        output.push_str("\n## Summary Anchor\n\n```text\n");
        output.push_str(summary.trim());
        output.push_str("\n```\n");
    }

    output.push_str("\n## Messages\n");
    for message in messages {
        output.push('\n');
        output.push_str("### ");
        output.push_str(role_label(&message.role));
        output.push('\n');

        if let Some(reasoning) = message.reasoning.as_deref() {
            output.push_str("\n**Reasoning**\n\n```text\n");
            output.push_str(reasoning.trim());
            output.push_str("\n```\n");
        }

        if let Some(content) = message.content.as_deref() {
            output.push_str("\n```text\n");
            output.push_str(content.trim());
            output.push_str("\n```\n");
        }

        if !message.tool_calls.is_empty() {
            output.push_str("\n**Tool Calls**\n\n```json\n");
            output.push_str(
                &serde_json::to_string_pretty(&message.tool_calls).unwrap_or_else(|_| "[]".into()),
            );
            output.push_str("\n```\n");
        }

        if let Some(tool_call_id) = message.tool_call_id.as_deref() {
            output.push_str(&format!("\nTool call id: `{}`\n", tool_call_id));
            if failed_tool_call_ids.contains(tool_call_id) {
                output.push_str("Tool result status: `error`\n");
            }
        }
    }

    output
}

fn short_session_id(session_id: &str) -> String {
    session_id.chars().take(8).collect()
}

fn role_label(role: &Role) -> &'static str {
    match role {
        Role::System => "System",
        Role::User => "User",
        Role::Assistant => "Assistant",
        Role::Tool => "Tool",
    }
}

#[derive(Debug, Default)]
struct FailedToolSummary {
    failed_tool_results: usize,
    failed_tool_names: Vec<String>,
}

fn summarize_failed_tools(
    messages: &[Message],
    failed_tool_call_ids: &HashSet<String>,
) -> FailedToolSummary {
    let mut tool_names_by_id = HashMap::new();
    for message in messages {
        for tool_call in &message.tool_calls {
            tool_names_by_id.insert(tool_call.id.as_str(), tool_call.name.as_str());
        }
    }

    let mut failed_tool_results = 0usize;
    let mut failed_tool_names = BTreeSet::new();
    for message in messages {
        if !matches!(message.role, Role::Tool) {
            continue;
        }
        let Some(tool_call_id) = message.tool_call_id.as_deref() else {
            continue;
        };
        if !failed_tool_call_ids.contains(tool_call_id) {
            continue;
        }
        failed_tool_results += 1;
        if let Some(name) = tool_names_by_id.get(tool_call_id) {
            failed_tool_names.insert((*name).to_string());
        }
    }

    FailedToolSummary {
        failed_tool_results,
        failed_tool_names: failed_tool_names.into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use tempfile::tempdir;
    use yode_llm::types::{Message, ToolCall};

    use super::write_compaction_transcript;
    use crate::context_manager::CompressionReport;

    #[test]
    fn writes_compaction_transcript_file() {
        let temp = tempdir().unwrap();
        let report = CompressionReport {
            removed: 4,
            tool_results_truncated: 1,
            summary: Some("summary anchor".to_string()),
        };

        let path = write_compaction_transcript(
            temp.path(),
            "session-1234",
            &[Message::user("hello"), Message::assistant("world")],
            &report,
            "auto",
            &HashSet::new(),
        )
        .unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("Compaction Transcript"));
        assert!(content.contains("- Mode: auto"));
        assert!(content.contains("- Failed tool results: 0"));
        assert!(content.contains("summary anchor"));
        assert!(content.contains("### User"));
        assert!(content.contains("### Assistant"));
    }

    #[test]
    fn writes_failed_tool_metadata_when_known() {
        let temp = tempdir().unwrap();
        let report = CompressionReport {
            removed: 2,
            tool_results_truncated: 0,
            summary: None,
        };

        let mut assistant = Message::assistant("Running diagnostics");
        assistant.tool_calls.push(ToolCall {
            id: "tc1".to_string(),
            name: "bash".to_string(),
            arguments: "{\"command\":\"false\"}".to_string(),
        });
        let messages = vec![
            assistant,
            Message::tool_result("tc1", "Tool execution failed: boom"),
        ];

        let path = write_compaction_transcript(
            temp.path(),
            "session-1234",
            &messages,
            &report,
            "manual",
            &HashSet::from(["tc1".to_string()]),
        )
        .unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("- Failed tool results: 1"));
        assert!(content.contains("- Failed tools: bash"));
        assert!(content.contains("Tool result status: `error`"));
    }
}
