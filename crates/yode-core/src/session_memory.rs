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
const MEMORY_WRITE_RETRIES: usize = 3;

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
    pub goals: Vec<String>,
    pub findings: Vec<String>,
    pub decisions: Vec<String>,
    pub open_questions: Vec<String>,
    pub files_read: Vec<String>,
    pub files_modified: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct StructuredMemorySections {
    goals: Vec<String>,
    findings: Vec<String>,
    decisions: Vec<String>,
    open_questions: Vec<String>,
}

#[derive(Debug, Clone)]
struct MemorySchemaHints {
    freshness: Vec<String>,
    confidence: Vec<String>,
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
    write_string_with_retry(&path, &content)
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
    write_string_with_retry(&path, &content).with_context(|| {
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
    let generated_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let hints = live_memory_hints(&generated_at);
    let summary_body = normalize_live_summary_markdown(summary, snapshot, &hints);
    content.push_str(&format!(
        "## {} session {}\n\n### Session Stats\n\n- Total tool calls this session: {}\n- Current message count: {}\n\n{}\n",
        generated_at,
        snapshot.session_id.chars().take(8).collect::<String>(),
        snapshot.total_tool_calls,
        snapshot.message_count,
        summary_body
    ));

    let content = truncate_memory_file(content);
    write_string_with_retry(&path, &content).with_context(|| {
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
    let generated_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let sections = structured_sections_from_compaction_summary(report.summary.as_deref());
    let hints = compaction_memory_hints(&generated_at);
    let files_read_summary = summarize_read_files(project_root, files_read);
    let files_modified_summary = summarize_modified_files(project_root, files_modified);
    let mut lines = vec![
        format!("## {} session {}", generated_at, short_session_id),
        String::new(),
        "- Trigger: auto_compact".to_string(),
        format!("- Removed messages: {}", report.removed),
        format!(
            "- Tool results truncated: {}",
            report.tool_results_truncated
        ),
        String::new(),
    ];
    render_structured_sections(
        &mut lines,
        &sections,
        files_read_summary.as_deref(),
        files_modified_summary.as_deref(),
        &hints,
    );

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

    let marker = "\n\n[Older session memory entries truncated]";
    let budget = MAX_SESSION_MEMORY_CHARS.saturating_sub(marker.chars().count());

    if let Some(first_entry_start) = content.find("\n\n## ") {
        let header = &content[..first_entry_start];
        let entries = &content[first_entry_start + 2..];
        let mut truncated = String::new();
        truncated.push_str(header);

        let mut remaining = budget.saturating_sub(header.chars().count());
        for (idx, entry) in entries.split("\n\n## ").enumerate() {
            let rendered_entry = if idx == 0 {
                entry.to_string()
            } else {
                format!("## {}", entry)
            };
            let entry_chars = rendered_entry.chars().count() + 2;
            if entry_chars > remaining {
                if idx == 0 {
                    let keep = remaining.saturating_sub(32);
                    let shortened = rendered_entry.chars().take(keep).collect::<String>();
                    truncated.push_str("\n\n");
                    truncated.push_str(&shortened);
                }
                break;
            }
            truncated.push_str("\n\n");
            truncated.push_str(&rendered_entry);
            remaining = remaining.saturating_sub(entry_chars);
        }

        truncated.push_str(marker);
        return truncated;
    }

    let mut truncated = content.chars().take(budget).collect::<String>();
    truncated.push_str(marker);
    truncated
}

fn write_string_with_retry(path: &Path, content: &str) -> Result<()> {
    let mut last_err = None;
    for attempt in 0..MEMORY_WRITE_RETRIES {
        match fs::write(path, content) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_err = Some(err);
                if attempt + 1 < MEMORY_WRITE_RETRIES {
                    std::thread::sleep(std::time::Duration::from_millis(25 * (attempt as u64 + 1)));
                }
            }
        }
    }
    Err(last_err.unwrap().into())
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
    let mut decisions = Vec::new();
    let mut open_questions = Vec::new();

    for message in messages.iter().rev() {
        match message.role {
            Role::User => {
                if let Some(content) = message.content.as_deref() {
                    push_unique_excerpt(&mut user_goals, content, 160, 3);
                    if looks_like_open_question(content) {
                        push_unique_excerpt(&mut open_questions, content, 180, 3);
                    }
                }
            }
            Role::Assistant if message.tool_calls.is_empty() => {
                if let Some(content) = message.content.as_deref() {
                    push_unique_excerpt(&mut assistant_findings, content, 180, 3);
                    if looks_like_decision(content) {
                        push_unique_excerpt(&mut decisions, content, 180, 3);
                    }
                    if looks_like_open_question(content) {
                        push_unique_excerpt(&mut open_questions, content, 180, 3);
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
        goals: user_goals,
        findings: assistant_findings,
        decisions,
        open_questions,
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
         1. Goals\n2. Findings\n3. Decisions\n4. Files\n5. Open Questions\n\n\
         Rules:\n\
         - Keep only verified facts and the current active direction.\n\
         - Prefer concrete file paths and technical constraints.\n\
         - Use `- None` for empty sections.\n\
         - Omit chatter, duplicated history, and completed low-value details.\n\
         - Keep the whole output under 260 words.\n\
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
    let generated_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut lines = vec![
        format!(
            "## {} session {}",
            generated_at,
            snapshot.session_id.chars().take(8).collect::<String>()
        ),
        String::new(),
        format!(
            "- Total tool calls this session: {}",
            snapshot.total_tool_calls
        ),
        format!("- Current message count: {}", snapshot.message_count),
        String::new(),
    ];
    let sections = StructuredMemorySections {
        goals: snapshot.goals.clone(),
        findings: snapshot.findings.clone(),
        decisions: snapshot.decisions.clone(),
        open_questions: snapshot.open_questions.clone(),
    };
    let files_read_summary = summarize_entries(snapshot.files_read.clone());
    let files_modified_summary = summarize_entries(snapshot.files_modified.clone());
    let hints = live_memory_hints(&generated_at);
    render_structured_sections(
        &mut lines,
        &sections,
        files_read_summary.as_deref(),
        files_modified_summary.as_deref(),
        &hints,
    );

    lines.join("\n")
}

fn render_structured_sections(
    lines: &mut Vec<String>,
    sections: &StructuredMemorySections,
    files_read_summary: Option<&str>,
    files_modified_summary: Option<&str>,
    hints: &MemorySchemaHints,
) {
    push_bullet_section(lines, "Goals", &sections.goals);
    push_bullet_section(lines, "Findings", &sections.findings);
    push_bullet_section(lines, "Decisions", &sections.decisions);

    lines.push("### Files".to_string());
    lines.push(String::new());
    let mut wrote_file_line = false;
    if let Some(read_summary) = files_read_summary {
        lines.push(format!("- Read: {}", read_summary));
        wrote_file_line = true;
    }
    if let Some(modified_summary) = files_modified_summary {
        lines.push(format!("- Modified: {}", modified_summary));
        wrote_file_line = true;
    }
    if !wrote_file_line {
        lines.push("- None".to_string());
    }
    lines.push(String::new());

    push_bullet_section(lines, "Open Questions", &sections.open_questions);
    push_bullet_section(lines, "Freshness", &hints.freshness);
    push_bullet_section(lines, "Confidence", &hints.confidence);
}

fn push_bullet_section(lines: &mut Vec<String>, title: &str, items: &[String]) {
    lines.push(format!("### {}", title));
    lines.push(String::new());
    if items.is_empty() {
        lines.push("- None".to_string());
    } else {
        for item in items {
            lines.push(format!("- {}", item));
        }
    }
    lines.push(String::new());
}

fn structured_sections_from_compaction_summary(summary: Option<&str>) -> StructuredMemorySections {
    let Some(summary) = summary else {
        return StructuredMemorySections {
            findings: vec![
                "Tool-result trimming reclaimed space without generating a summary anchor."
                    .to_string(),
            ],
            ..Default::default()
        };
    };

    let mut sections = StructuredMemorySections::default();
    for line in summary.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(value) = trimmed.strip_prefix("[Context summary]") {
            let value = value.trim();
            if !value.is_empty() {
                sections.findings.push(value.to_string());
            }
        } else if let Some(value) = trimmed.strip_prefix("- Earlier user goals: ") {
            sections.goals.extend(split_pipe_items(value));
        } else if let Some(value) = trimmed.strip_prefix("- Earlier assistant findings: ") {
            sections.findings.extend(split_pipe_items(value));
        } else if let Some(value) = trimmed.strip_prefix("- Earlier tool activity: ") {
            sections
                .findings
                .push(format!("Tool activity: {}", value.trim()));
        } else if let Some(value) = trimmed.strip_prefix("- Tool results compacted: ") {
            sections
                .findings
                .push(format!("Tool results compacted: {}", value.trim()));
        }
    }

    if sections.goals.is_empty() && sections.findings.is_empty() {
        sections.findings.push(summary.trim().to_string());
    }

    dedupe_section_items(&mut sections.goals);
    dedupe_section_items(&mut sections.findings);
    dedupe_section_items(&mut sections.decisions);
    dedupe_section_items(&mut sections.open_questions);
    sections
}

fn live_memory_hints(generated_at: &str) -> MemorySchemaHints {
    MemorySchemaHints {
        freshness: vec![
            format!("Generated at: {}", generated_at),
            "Current-session snapshot; prefer this over older compacted entries.".to_string(),
        ],
        confidence: vec![
            "High for goals/files; medium for inferred findings and decisions.".to_string(),
            "Derived from direct recent session messages and file activity.".to_string(),
        ],
    }
}

fn compaction_memory_hints(generated_at: &str) -> MemorySchemaHints {
    MemorySchemaHints {
        freshness: vec![
            format!("Generated at: {}", generated_at),
            "Point-in-time compact snapshot; verify against current code if the session has moved on."
                .to_string(),
        ],
        confidence: vec![
            "Medium; synthesized from compaction summary plus current-turn file activity."
                .to_string(),
            "Use transcript artifacts when a removed detail needs exact recovery.".to_string(),
        ],
    }
}

fn normalize_live_summary_markdown(
    summary: &str,
    snapshot: &LiveSessionSnapshot,
    hints: &MemorySchemaHints,
) -> String {
    let trimmed = summary.trim();
    if trimmed.contains("### Goals") || trimmed.contains("### Findings") {
        let mut output = trimmed.to_string();
        if !trimmed.contains("### Freshness") {
            if !output.ends_with('\n') {
                output.push('\n');
            }
            output.push('\n');
            output.push_str(&render_named_section("Freshness", &hints.freshness));
        }
        if !trimmed.contains("### Confidence") {
            if !output.ends_with('\n') {
                output.push('\n');
            }
            output.push('\n');
            output.push_str(&render_named_section("Confidence", &hints.confidence));
        }
        return output;
    }

    let sections = StructuredMemorySections {
        goals: snapshot.goals.clone(),
        findings: vec![trimmed.to_string()],
        decisions: snapshot.decisions.clone(),
        open_questions: snapshot.open_questions.clone(),
    };
    let files_read_summary = summarize_entries(snapshot.files_read.clone());
    let files_modified_summary = summarize_entries(snapshot.files_modified.clone());
    let mut lines = Vec::new();
    render_structured_sections(
        &mut lines,
        &sections,
        files_read_summary.as_deref(),
        files_modified_summary.as_deref(),
        hints,
    );
    lines.join("\n")
}

fn render_named_section(title: &str, items: &[String]) -> String {
    let mut lines = vec![format!("### {}", title), String::new()];
    if items.is_empty() {
        lines.push("- None".to_string());
    } else {
        for item in items {
            lines.push(format!("- {}", item));
        }
    }
    lines.join("\n")
}

fn split_pipe_items(value: &str) -> Vec<String> {
    value
        .split('|')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn dedupe_section_items(items: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    items.retain(|item| seen.insert(item.clone()));
}

fn push_unique_excerpt(target: &mut Vec<String>, content: &str, limit: usize, max_items: usize) {
    if target.len() >= max_items {
        return;
    }
    if let Some(excerpt) = excerpt(content, limit) {
        if !target.contains(&excerpt) {
            target.push(excerpt);
        }
    }
}

fn looks_like_decision(content: &str) -> bool {
    let normalized = content.trim().to_lowercase();
    normalized.starts_with("i will ")
        || normalized.starts_with("we will ")
        || normalized.starts_with("we'll ")
        || normalized.starts_with("use ")
        || normalized.starts_with("keep ")
        || normalized.starts_with("switch ")
        || normalized.starts_with("prefer ")
        || normalized.contains(" decided ")
        || normalized.contains(" decision ")
        || normalized.contains(" plan is ")
}

fn looks_like_open_question(content: &str) -> bool {
    let normalized = content.to_lowercase();
    content.contains('?')
        || normalized.contains("not sure")
        || normalized.contains("unknown")
        || normalized.contains("unclear")
        || normalized.contains("need to verify")
        || normalized.contains("follow up")
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
        persist_live_session_memory, persist_live_session_memory_summary,
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
        assert!(content.contains("### Goals"));
        assert!(content.contains("### Findings"));
        assert!(content.contains("### Decisions"));
        assert!(content.contains("### Files"));
        assert!(content.contains("### Open Questions"));
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
        assert!(content.contains("### Files"));
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
        assert!(content.contains("### Goals"));
        assert!(content.contains("### Findings"));
        assert!(content.contains("### Decisions"));
        assert!(content.contains("### Files"));
        assert!(content.contains("### Open Questions"));
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

    #[test]
    fn normalizes_unstructured_live_summary_into_schema() {
        let temp = tempfile::tempdir().unwrap();
        let snapshot = build_live_snapshot(
            "session-live",
            &[
                Message::user("Investigate the resume bug"),
                Message::assistant("I will keep the persisted snapshot approach."),
            ],
            2,
            &[temp.path().join("src/lib.rs").display().to_string()],
            &[temp.path().join("src/main.rs").display().to_string()],
        );

        let path = persist_live_session_memory_summary(
            temp.path(),
            &snapshot,
            "Need to preserve the snapshot rewrite fix.",
        )
        .unwrap();
        let content = std::fs::read_to_string(path).unwrap();

        assert!(content.contains("### Goals"));
        assert!(content.contains("### Findings"));
        assert!(content.contains("### Decisions"));
        assert!(content.contains("### Files"));
        assert!(content.contains("### Open Questions"));
        assert!(content.contains("### Freshness"));
        assert!(content.contains("### Confidence"));
    }
}
