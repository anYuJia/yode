use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use yode_llm::types::{Message, Role};

pub(super) fn render_summary_anchor(summary: Option<&str>) -> String {
    summary
        .map(|summary| format!("\n## Summary Anchor\n\n```text\n{}\n```\n", summary.trim()))
        .unwrap_or_default()
}

pub(super) fn summarize_failed_tools(
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

#[derive(Debug, Default)]
pub(super) struct FailedToolSummary {
    pub failed_tool_results: usize,
    pub failed_tool_names: Vec<String>,
}

pub(super) fn summarize_read_files(
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

pub(super) fn summarize_modified_files(
    project_root: &Path,
    files_modified: &[String],
) -> Option<String> {
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
