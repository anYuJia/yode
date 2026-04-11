use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use yode_llm::types::{Message, Role};

use crate::context_manager::CompressionReport;

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

fn summarize_read_files(
    project_root: &Path,
    files_read: &HashMap<String, usize>,
) -> Option<String> {
    if files_read.is_empty() {
        return None;
    }
    let mut entries = files_read
        .iter()
        .map(|(path, lines)| format!("{} ({} lines)", display_path(project_root, path), lines))
        .collect::<Vec<_>>();
    entries.sort();
    summarize_entries(entries)
}

fn summarize_modified_files(project_root: &Path, files_modified: &[String]) -> Option<String> {
    if files_modified.is_empty() {
        return None;
    }
    let mut entries = files_modified
        .iter()
        .map(|path| display_path(project_root, path))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    entries.sort();
    summarize_entries(entries)
}

fn summarize_entries(mut entries: Vec<String>) -> Option<String> {
    if entries.is_empty() {
        return None;
    }
    const MAX_LISTED_FILES: usize = 8;
    let extra = entries.len().saturating_sub(MAX_LISTED_FILES);
    entries.truncate(MAX_LISTED_FILES);
    let mut summary = entries.join(", ");
    if extra > 0 {
        summary.push_str(&format!(", +{} more", extra));
    }
    Some(summary)
}

fn display_path(project_root: &Path, raw_path: &str) -> String {
    let path = Path::new(raw_path);
    if let Ok(relative) = path.strip_prefix(project_root) {
        return relative.display().to_string();
    }
    raw_path.to_string()
}
