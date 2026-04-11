use super::*;

pub fn session_memory_path(project_root: &Path) -> PathBuf {
    project_root.join(SESSION_MEMORY_RELATIVE_PATH)
}

pub fn live_session_memory_path(project_root: &Path) -> PathBuf {
    project_root.join(LIVE_SESSION_MEMORY_RELATIVE_PATH)
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
    content.push_str(&super::snapshot::render_live_snapshot(snapshot));

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
    let hints = super::schema::live_memory_hints(&generated_at);
    let summary_body = super::schema::normalize_live_summary_markdown(summary, snapshot, &hints);
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
    let sections =
        super::schema::structured_sections_from_compaction_summary(report.summary.as_deref());
    let hints = super::schema::compaction_memory_hints(&generated_at);
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
    super::schema::render_structured_sections(
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

pub(in crate::session_memory) fn summarize_entries(mut entries: Vec<String>) -> Option<String> {
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
