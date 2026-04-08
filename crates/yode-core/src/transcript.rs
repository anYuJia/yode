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
        render_compaction_transcript(session_id, messages, report, mode),
    )
    .with_context(|| format!("Failed to write transcript file: {}", path.display()))?;

    Ok(path)
}

fn render_compaction_transcript(
    session_id: &str,
    messages: &[Message],
    report: &CompressionReport,
    mode: &str,
) -> String {
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

#[cfg(test)]
mod tests {
    use tempfile::tempdir;
    use yode_llm::types::Message;

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
        )
        .unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("Compaction Transcript"));
        assert!(content.contains("- Mode: auto"));
        assert!(content.contains("summary anchor"));
        assert!(content.contains("### User"));
        assert!(content.contains("### Assistant"));
    }
}
