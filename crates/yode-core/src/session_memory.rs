use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use yode_llm::types::{Message, Role};

use crate::context_manager::CompressionReport;

const SESSION_MEMORY_RELATIVE_PATH: &str = ".yode/memory/session.md";
const LIVE_SESSION_MEMORY_RELATIVE_PATH: &str = ".yode/memory/session.live.md";
const SESSION_MEMORY_HEADER: &str = "# Session Memory\n\nYode writes this file automatically after context compaction. Newer entries appear first.";
const LIVE_SESSION_MEMORY_HEADER: &str =
    "# Session Snapshot\n\nYode refreshes this file during the session to preserve recent context between compactions.";
const MAX_SESSION_MEMORY_CHARS: usize = 16_000;
const MAX_LISTED_FILES: usize = 8;

pub fn session_memory_path(project_root: &Path) -> PathBuf {
    project_root.join(SESSION_MEMORY_RELATIVE_PATH)
}

pub fn live_session_memory_path(project_root: &Path) -> PathBuf {
    project_root.join(LIVE_SESSION_MEMORY_RELATIVE_PATH)
}

#[derive(Debug, Clone)]
pub struct LiveSessionSnapshot {
    pub session_id: String,
    pub total_tool_calls: u32,
    pub message_count: usize,
    pub user_goals: Vec<String>,
    pub assistant_findings: Vec<String>,
    pub files_read: Vec<String>,
    pub files_modified: Vec<String>,
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

pub fn persist_live_session_memory(
    project_root: &Path,
    snapshot: &LiveSessionSnapshot,
) -> Result<PathBuf> {
    let path = live_session_memory_path(project_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create live session memory directory: {}",
                parent.display()
            )
        })?;
    }

    let mut content = String::new();
    content.push_str(LIVE_SESSION_MEMORY_HEADER);
    content.push_str("\n\n");
    content.push_str(&render_live_snapshot(snapshot));

    let content = truncate_memory_file(content);
    fs::write(&path, content).with_context(|| {
        format!(
            "Failed to write live session memory file: {}",
            path.display()
        )
    })?;

    Ok(path)
}

pub fn persist_live_session_memory_summary(
    project_root: &Path,
    snapshot: &LiveSessionSnapshot,
    summary: &str,
) -> Result<PathBuf> {
    let path = live_session_memory_path(project_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create live session memory directory: {}",
                parent.display()
            )
        })?;
    }

    let mut content = String::new();
    content.push_str(LIVE_SESSION_MEMORY_HEADER);
    content.push_str("\n\n");
    content.push_str(&format!(
        "## {} session {}\n\n- Total tool calls this session: {}\n- Current message count: {}\n\n{}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        snapshot.session_id.chars().take(8).collect::<String>(),
        snapshot.total_tool_calls,
        snapshot.message_count,
        summary.trim()
    ));

    let content = truncate_memory_file(content);
    fs::write(&path, content).with_context(|| {
        format!(
            "Failed to write live session memory file: {}",
            path.display()
        )
    })?;

    Ok(path)
}

pub fn clear_live_session_memory(project_root: &Path) -> Result<()> {
    let path = live_session_memory_path(project_root);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err)
            .with_context(|| format!("Failed to remove live session memory: {}", path.display())),
    }
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

pub fn build_live_snapshot(
    session_id: &str,
    messages: &[Message],
    total_tool_calls: u32,
    files_read: &[String],
    files_modified: &[String],
) -> LiveSessionSnapshot {
    let mut user_goals = Vec::new();
    let mut assistant_findings = Vec::new();

    for message in messages.iter().rev() {
        match message.role {
            Role::User => {
                if user_goals.len() < 3 {
                    if let Some(content) = message.content.as_deref() {
                        if let Some(excerpt) = excerpt(content, 160) {
                            if !user_goals.contains(&excerpt) {
                                user_goals.push(excerpt);
                            }
                        }
                    }
                }
            }
            Role::Assistant if message.tool_calls.is_empty() => {
                if assistant_findings.len() < 3 {
                    if let Some(content) = message.content.as_deref() {
                        if let Some(excerpt) = excerpt(content, 180) {
                            if !assistant_findings.contains(&excerpt) {
                                assistant_findings.push(excerpt);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    LiveSessionSnapshot {
        session_id: session_id.to_string(),
        total_tool_calls,
        message_count: messages.len(),
        user_goals,
        assistant_findings,
        files_read: dedupe_entries(files_read),
        files_modified: dedupe_entries(files_modified),
    }
}

pub fn render_live_session_memory_prompt(
    existing_summary: Option<&str>,
    snapshot: &LiveSessionSnapshot,
    recent_messages: &[Message],
) -> String {
    let mut prompt = String::new();
    prompt.push_str(
        "Update the session memory for an AI coding assistant.\n\
         Produce concise markdown with these sections in order:\n\
         1. Goals\n2. Findings\n3. Files\n4. Open Questions\n\n\
         Rules:\n\
         - Keep only verified facts and the current active direction.\n\
         - Prefer concrete file paths and technical constraints.\n\
         - Omit chatter, duplicated history, and completed low-value details.\n\
         - Keep the whole output under 220 words.\n\
         - Return markdown only.\n\n",
    );

    if let Some(existing) = existing_summary {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            prompt.push_str("Existing session memory:\n```md\n");
            prompt.push_str(trimmed);
            prompt.push_str("\n```\n\n");
        }
    }

    prompt.push_str("Deterministic snapshot:\n");
    prompt.push_str(&render_live_snapshot(snapshot));
    prompt.push_str("\n\nRecent messages:\n");
    prompt.push_str(&format_recent_messages(recent_messages));
    prompt
}

fn render_live_snapshot(snapshot: &LiveSessionSnapshot) -> String {
    let mut lines = vec![
        format!(
            "## {} session {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            snapshot.session_id.chars().take(8).collect::<String>()
        ),
        String::new(),
        format!(
            "- Total tool calls this session: {}",
            snapshot.total_tool_calls
        ),
        format!("- Current message count: {}", snapshot.message_count),
    ];

    if !snapshot.user_goals.is_empty() {
        lines.push(format!(
            "- Recent user goals: {}",
            snapshot.user_goals.join(" | ")
        ));
    }

    if !snapshot.assistant_findings.is_empty() {
        lines.push(format!(
            "- Recent assistant findings: {}",
            snapshot.assistant_findings.join(" | ")
        ));
    }

    if !snapshot.files_read.is_empty() {
        lines.push(format!(
            "- Recently read files: {}",
            summarize_entries(snapshot.files_read.clone()).unwrap_or_default()
        ));
    }

    if !snapshot.files_modified.is_empty() {
        lines.push(format!(
            "- Recently modified files: {}",
            summarize_entries(snapshot.files_modified.clone()).unwrap_or_default()
        ));
    }

    lines.join("\n")
}

fn format_recent_messages(messages: &[Message]) -> String {
    let mut lines = Vec::new();
    for message in messages {
        let role = match message.role {
            Role::System => "System",
            Role::User => "User",
            Role::Assistant => "Assistant",
            Role::Tool => "Tool",
        };

        if let Some(content) = message.content.as_deref() {
            if let Some(excerpt) = excerpt(content, 220) {
                lines.push(format!("{}: {}", role, excerpt));
            }
        }
    }

    lines.join("\n")
}

fn excerpt(text: &str, limit: usize) -> Option<String> {
    let squashed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if squashed.is_empty() {
        return None;
    }

    let shortened: String = squashed.chars().take(limit).collect();
    if squashed.chars().count() > limit {
        Some(format!("{}...", shortened.trim_end()))
    } else {
        Some(shortened)
    }
}

fn dedupe_entries(entries: &[String]) -> Vec<String> {
    entries
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{
        build_live_snapshot, clear_live_session_memory, persist_compaction_memory,
        persist_live_session_memory,
    };
    use crate::context_manager::CompressionReport;
    use yode_llm::types::Message;

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

    #[test]
    fn persists_live_session_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let snapshot = build_live_snapshot(
            "session-live",
            &[
                Message::user("Investigate the resume bug in compact mode"),
                Message::assistant("I traced it to the persisted message snapshot."),
            ],
            4,
            &[temp.path().join("src/lib.rs").display().to_string()],
            &[temp.path().join("src/main.rs").display().to_string()],
        );

        let path = persist_live_session_memory(temp.path(), &snapshot).unwrap();
        let content = std::fs::read_to_string(path).unwrap();

        assert!(content.contains("Session Snapshot"));
        assert!(content.contains("resume bug"));
        assert!(content.contains("persisted message snapshot"));
        assert!(content.contains("Total tool calls this session: 4"));
    }

    #[test]
    fn clears_live_session_snapshot_file() {
        let temp = tempfile::tempdir().unwrap();
        let snapshot = build_live_snapshot("session-live", &[Message::user("hello")], 1, &[], &[]);
        let path = persist_live_session_memory(temp.path(), &snapshot).unwrap();
        assert!(path.exists());

        clear_live_session_memory(temp.path()).unwrap();
        assert!(!path.exists());
    }
}
