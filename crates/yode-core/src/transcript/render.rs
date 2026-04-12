use std::collections::{HashMap, HashSet};
use std::path::Path;

use yode_llm::types::{Message, Role};

use crate::context_manager::CompressionReport;
use super::summary::{
    render_summary_anchor, summarize_failed_tools, summarize_modified_files, summarize_read_files,
};

pub(super) fn render_compaction_transcript(
    project_root: &Path,
    session_id: &str,
    messages: &[Message],
    report: &CompressionReport,
    mode: &str,
    failed_tool_call_ids: &HashSet<String>,
    session_memory_path: Option<&Path>,
    files_read: &HashMap<String, usize>,
    files_modified: &[String],
) -> String {
    let failure_summary = summarize_failed_tools(messages, failed_tool_call_ids);
    let files_read_summary = summarize_read_files(project_root, files_read);
    let files_modified_summary = summarize_modified_files(project_root, files_modified);
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
    if let Some(path) = session_memory_path {
        output.push_str(&format!("- Session memory path: {}\n", path.display()));
    }
    if let Some(summary) = files_read_summary {
        output.push_str(&format!("- Files read: {}\n", summary));
    }
    if let Some(summary) = files_modified_summary {
        output.push_str(&format!("- Files modified: {}\n", summary));
    }
    output.push_str(&render_summary_anchor(report.summary.as_deref()));

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

pub(super) fn short_session_id(session_id: &str) -> String {
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
