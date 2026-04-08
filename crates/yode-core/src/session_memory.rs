use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::context_manager::CompressionReport;

const SESSION_MEMORY_RELATIVE_PATH: &str = ".yode/memory/session.md";
const SESSION_MEMORY_HEADER: &str = "# Session Memory\n\nYode writes this file automatically after context compaction. Newer entries appear first.";
const MAX_SESSION_MEMORY_CHARS: usize = 16_000;
const MAX_LISTED_FILES: usize = 8;

pub fn session_memory_path(project_root: &Path) -> PathBuf {
    project_root.join(SESSION_MEMORY_RELATIVE_PATH)
}

pub fn persist_compaction_memory(
    project_root: &Path,
    session_id: &str,
    report: &CompressionReport,
    files_read: &HashMap<String, usize>,
    files_modified: &[String],
) -> Result<PathBuf> {
    let path = session_memory_path(project_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create session memory directory: {}",
                parent.display()
            )
        })?;
    }

    let previous = fs::read_to_string(&path).unwrap_or_default();
    let existing_entries = previous
        .strip_prefix(SESSION_MEMORY_HEADER)
        .map(str::trim)
        .unwrap_or_else(|| previous.trim());

    let mut content = String::new();
    content.push_str(SESSION_MEMORY_HEADER);
    content.push_str("\n\n");
    content.push_str(&render_entry(
        project_root,
        session_id,
        report,
        files_read,
        files_modified,
    ));

    if !existing_entries.is_empty() {
        content.push_str("\n\n");
        content.push_str(existing_entries);
    }

    let content = truncate_memory_file(content);
    fs::write(&path, content)
        .with_context(|| format!("Failed to write session memory file: {}", path.display()))?;

    Ok(path)
}

fn render_entry(
    project_root: &Path,
    session_id: &str,
    report: &CompressionReport,
    files_read: &HashMap<String, usize>,
    files_modified: &[String],
) -> String {
    let short_session_id: String = session_id.chars().take(8).collect();
    let mut lines = vec![
        format!(
            "## {} session {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            short_session_id
        ),
        String::new(),
        "- Trigger: auto_compact".to_string(),
        format!("- Removed messages: {}", report.removed),
        format!(
            "- Tool results truncated: {}",
            report.tool_results_truncated
        ),
    ];

    if let Some(read_summary) = summarize_read_files(project_root, files_read) {
        lines.push(format!("- Files read in current turn: {}", read_summary));
    }

    if let Some(modified_summary) = summarize_modified_files(project_root, files_modified) {
        lines.push(format!(
            "- Files modified in current turn: {}",
            modified_summary
        ));
    }

    lines.push(String::new());
    lines.push("Summary:".to_string());

    if let Some(summary) = report.summary.as_deref() {
        lines.push("```text".to_string());
        lines.push(summary.trim().to_string());
        lines.push("```".to_string());
    } else {
        lines.push(
            "Tool-result trimming reclaimed space without generating a summary anchor.".to_string(),
        );
    }

    lines.join("\n")
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

fn truncate_memory_file(content: String) -> String {
    if content.chars().count() <= MAX_SESSION_MEMORY_CHARS {
        return content;
    }

    let mut truncated = content
        .chars()
        .take(MAX_SESSION_MEMORY_CHARS.saturating_sub(40))
        .collect::<String>();

    if let Some(last_entry) = truncated.rfind("\n## ") {
        if last_entry > SESSION_MEMORY_HEADER.len() {
            truncated.truncate(last_entry);
        }
    }

    truncated.push_str("\n\n[Older session memory entries truncated]");
    truncated
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::persist_compaction_memory;
    use crate::context_manager::CompressionReport;

    #[test]
    fn prepends_newer_session_memory_entries() {
        let temp = tempfile::tempdir().unwrap();
        let project_root = temp.path();

        let first = CompressionReport {
            removed: 3,
            tool_results_truncated: 1,
            summary: Some("first summary".to_string()),
        };
        let second = CompressionReport {
            removed: 7,
            tool_results_truncated: 0,
            summary: Some("second summary".to_string()),
        };

        persist_compaction_memory(project_root, "session-one", &first, &HashMap::new(), &[])
            .unwrap();
        let path =
            persist_compaction_memory(project_root, "session-two", &second, &HashMap::new(), &[])
                .unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        let first_idx = content.find("first summary").unwrap();
        let second_idx = content.find("second summary").unwrap();
        assert!(second_idx < first_idx);
    }

    #[test]
    fn includes_relative_file_summaries() {
        let temp = tempfile::tempdir().unwrap();
        let project_root = temp.path();

        let report = CompressionReport {
            removed: 5,
            tool_results_truncated: 2,
            summary: Some("summary".to_string()),
        };

        let mut files_read = HashMap::new();
        files_read.insert(
            project_root.join("src/lib.rs").display().to_string(),
            120usize,
        );

        let path = persist_compaction_memory(
            project_root,
            "session-three",
            &report,
            &files_read,
            &[project_root.join("src/main.rs").display().to_string()],
        )
        .unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("src/lib.rs (120 lines)"));
        assert!(content.contains("src/main.rs"));
    }
}
