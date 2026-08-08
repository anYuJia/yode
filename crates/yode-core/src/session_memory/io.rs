use super::*;
use crate::session_artifact::{
    atomic_write_async, atomic_write_sync, json_artifact_session_id, markdown_artifact_session_id,
    markdown_front_matter_session_id, session_artifact_token, verify_memory_ownership,
    ArtifactOwnership,
};
use tracing::{info, warn};

/// 当前会话的压缩记忆文件路径（按完整 session id 隔离）。
pub fn session_memory_path(project_root: &Path, session_id: &str) -> PathBuf {
    project_root
        .join(".yode")
        .join("memory")
        .join(format!("session-{}.md", session_artifact_token(session_id)))
}

/// 当前会话的 live 记忆文件路径（按完整 session id 隔离）。
pub fn live_session_memory_path(project_root: &Path, session_id: &str) -> PathBuf {
    project_root.join(".yode").join("memory").join(format!(
        "session-{}.live.md",
        session_artifact_token(session_id)
    ))
}

fn legacy_session_memory_path(project_root: &Path) -> PathBuf {
    project_root.join(SESSION_MEMORY_RELATIVE_PATH)
}

fn legacy_live_session_memory_path(project_root: &Path) -> PathBuf {
    project_root.join(LIVE_SESSION_MEMORY_RELATIVE_PATH)
}

/// 一次性迁移旧共享 memory 到当前会话：
/// 仅当旧文件全部条目都能验证归属当前 session（完整 id 或旧版 8 位前缀）时迁移；
/// 无法验证时不读取、不删除。
pub fn migrate_legacy_session_memory(project_root: &Path, session_id: &str) -> Result<()> {
    migrate_legacy_memory_file(
        session_id,
        &legacy_session_memory_path(project_root),
        &session_memory_path(project_root, session_id),
        "压缩记忆",
    )?;
    migrate_legacy_memory_file(
        session_id,
        &legacy_live_session_memory_path(project_root),
        &live_session_memory_path(project_root, session_id),
        "live 记忆",
    )
}

fn migrate_legacy_memory_file(
    session_id: &str,
    legacy: &Path,
    target: &Path,
    label: &str,
) -> Result<()> {
    let content = match fs::read_to_string(legacy) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            warn!(
                "无法读取旧共享{}文件 {} 进行迁移: {}",
                label,
                legacy.display(),
                err
            );
            return Ok(());
        }
    };

    match verify_memory_ownership(&content, session_id) {
        ArtifactOwnership::Exact | ArtifactOwnership::LegacyPrefix => {}
        ArtifactOwnership::Unverifiable => {
            warn!(
                "旧共享{}文件 {} 无法验证归属当前 session {}，跳过迁移（不读取、不删除）",
                label,
                legacy.display(),
                session_id
            );
            return Ok(());
        }
    }

    if target.exists() {
        info!(
            "已存在会话专属{}文件 {}，跳过旧共享文件迁移 {}",
            label,
            target.display(),
            legacy.display()
        );
        return Ok(());
    }

    let mut migrated = String::new();
    let header = if label == "live 记忆" {
        LIVE_SESSION_MEMORY_HEADER
    } else {
        SESSION_MEMORY_HEADER
    };
    migrated.push_str(header);
    migrated.push_str("\n\n- Session: ");
    migrated.push_str(session_id);
    migrated.push_str("\n\n");

    let body = content
        .strip_prefix(SESSION_MEMORY_HEADER)
        .or_else(|| content.strip_prefix(LIVE_SESSION_MEMORY_HEADER))
        .unwrap_or(&content)
        .trim();
    migrated.push_str(body);

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("无法创建会话记忆目录: {}", parent.display()))?;
    }
    atomic_write_sync(target, &migrated)
        .with_context(|| format!("迁移旧共享{}失败，无法写入 {}", label, target.display()))?;
    fs::remove_file(legacy).with_context(|| {
        format!(
            "旧共享{}迁移完成但删除失败 {}（不影响本次读取）",
            label,
            legacy.display()
        )
    })?;
    info!(
        "已迁移旧共享{}文件 {} 到会话专属文件 {}",
        label,
        legacy.display(),
        target.display()
    );
    Ok(())
}

/// 读取当前会话可用的压缩记忆摘要（优先 live，其次压缩记忆）。
/// 只返回验证归属当前 session 的工件；无法验证时拒绝恢复并记录诊断。
pub fn best_compaction_memory_excerpt(
    project_root: &Path,
    session_id: &str,
    max_chars: usize,
) -> Option<(PathBuf, String)> {
    migrate_legacy_session_memory(project_root, session_id).unwrap_or_else(|err| {
        warn!("旧共享记忆迁移失败（继续按新路径读取）: {}", err);
    });

    for path in [
        live_session_memory_path(project_root, session_id),
        session_memory_path(project_root, session_id),
    ] {
        if let Some(excerpt) = load_memory_excerpt(&path, session_id, max_chars) {
            if !excerpt.trim().is_empty() {
                return Some((path, excerpt));
            }
        }
    }

    None
}

fn session_memory_header_for(session_id: &str) -> String {
    format!("{SESSION_MEMORY_HEADER}\n\n- Session: {session_id}")
}

fn live_session_memory_header_for(session_id: &str) -> String {
    format!("{LIVE_SESSION_MEMORY_HEADER}\n\n- Session: {session_id}")
}

pub fn persist_compaction_memory(
    project_root: &Path,
    session_id: &str,
    report: &CompressionReport,
    files_read: &HashMap<String, usize>,
    files_modified: &[String],
) -> Result<PathBuf> {
    migrate_legacy_session_memory(project_root, session_id).unwrap_or_else(|err| {
        warn!("旧共享记忆迁移失败（继续按新路径写入）: {}", err);
    });
    let path = session_memory_path(project_root, session_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create session memory directory: {}",
                parent.display()
            )
        })?;
    }

    let previous = read_existing_session_memory(&path)?;
    let existing_entries = previous
        .strip_prefix(&session_memory_header_for(session_id))
        .map(str::trim)
        .unwrap_or_else(|| previous.trim());

    let mut content = String::new();
    content.push_str(&session_memory_header_for(session_id));
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
    atomic_write_sync(&path, &content)
        .with_context(|| format!("Failed to write session memory file: {}", path.display()))?;

    Ok(path)
}

pub async fn persist_compaction_memory_async(
    project_root: &Path,
    session_id: &str,
    report: &CompressionReport,
    files_read: &HashMap<String, usize>,
    files_modified: &[String],
) -> Result<PathBuf> {
    migrate_legacy_session_memory(project_root, session_id).unwrap_or_else(|err| {
        warn!("旧共享记忆迁移失败（继续按新路径写入）: {}", err);
    });
    let path = session_memory_path(project_root, session_id);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.with_context(|| {
            format!(
                "Failed to create session memory directory: {}",
                parent.display()
            )
        })?;
    }

    let previous = read_existing_session_memory_async(&path).await?;
    let existing_entries = previous
        .strip_prefix(&session_memory_header_for(session_id))
        .map(str::trim)
        .unwrap_or_else(|| previous.trim());

    let content = render_compaction_memory_content(
        project_root,
        session_id,
        report,
        files_read,
        files_modified,
        existing_entries,
    );
    atomic_write_async(&path, &content)
        .await
        .with_context(|| format!("Failed to write session memory file: {}", path.display()))?;

    Ok(path)
}

pub fn persist_live_session_memory(
    project_root: &Path,
    snapshot: &LiveSessionSnapshot,
) -> Result<PathBuf> {
    let path = live_session_memory_path(project_root, &snapshot.session_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create live session memory directory: {}",
                parent.display()
            )
        })?;
    }

    let mut content = String::new();
    content.push_str(&live_session_memory_header_for(&snapshot.session_id));
    content.push_str("\n\n");
    content.push_str(&super::snapshot::render_live_snapshot(snapshot));

    let content = truncate_memory_file(content);
    atomic_write_sync(&path, &content).with_context(|| {
        format!(
            "Failed to write live session memory file: {}",
            path.display()
        )
    })?;

    Ok(path)
}

pub async fn persist_live_session_memory_async(
    project_root: &Path,
    snapshot: &LiveSessionSnapshot,
) -> Result<PathBuf> {
    let path = live_session_memory_path(project_root, &snapshot.session_id);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.with_context(|| {
            format!(
                "Failed to create live session memory directory: {}",
                parent.display()
            )
        })?;
    }

    let content = render_live_session_memory_content(snapshot);
    atomic_write_async(&path, &content).await.with_context(|| {
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
    let path = live_session_memory_path(project_root, &snapshot.session_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create live session memory directory: {}",
                parent.display()
            )
        })?;
    }

    let mut content = String::new();
    content.push_str(&live_session_memory_header_for(&snapshot.session_id));
    content.push_str("\n\n");
    let generated_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let hints = super::schema::live_memory_hints(&generated_at);
    let summary_body = super::schema::normalize_live_summary_markdown(summary, snapshot, &hints);
    content.push_str(&format!(
        "## {} session {}\n\n### Session Stats\n\n- Total tool calls this session: {}\n- Current message count: {}\n\n{}\n",
        generated_at,
        snapshot.session_id,
        snapshot.total_tool_calls,
        snapshot.message_count,
        summary_body
    ));

    let content = truncate_memory_file(content);
    atomic_write_sync(&path, &content).with_context(|| {
        format!(
            "Failed to write live session memory file: {}",
            path.display()
        )
    })?;

    Ok(path)
}

pub async fn persist_live_session_memory_summary_async(
    project_root: &Path,
    snapshot: &LiveSessionSnapshot,
    summary: &str,
) -> Result<PathBuf> {
    let path = live_session_memory_path(project_root, &snapshot.session_id);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.with_context(|| {
            format!(
                "Failed to create live session memory directory: {}",
                parent.display()
            )
        })?;
    }

    let content = render_live_session_memory_summary_content(snapshot, summary);
    atomic_write_async(&path, &content).await.with_context(|| {
        format!(
            "Failed to write live session memory file: {}",
            path.display()
        )
    })?;

    Ok(path)
}

/// 仅删除当前会话自己的 live 记忆文件（按完整 session id 隔离）。
pub fn clear_live_session_memory(project_root: &Path, session_id: &str) -> Result<()> {
    let path = live_session_memory_path(project_root, session_id);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err)
            .with_context(|| format!("Failed to remove live session memory: {}", path.display())),
    }
}

fn render_compaction_memory_content(
    project_root: &Path,
    session_id: &str,
    report: &CompressionReport,
    files_read: &HashMap<String, usize>,
    files_modified: &[String],
    existing_entries: &str,
) -> String {
    let mut content = String::new();
    content.push_str(&session_memory_header_for(session_id));
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

    truncate_memory_file(content)
}

fn render_live_session_memory_content(snapshot: &LiveSessionSnapshot) -> String {
    let mut content = String::new();
    content.push_str(&live_session_memory_header_for(&snapshot.session_id));
    content.push_str("\n\n");
    content.push_str(&super::snapshot::render_live_snapshot(snapshot));
    truncate_memory_file(content)
}

fn render_live_session_memory_summary_content(
    snapshot: &LiveSessionSnapshot,
    summary: &str,
) -> String {
    let mut content = String::new();
    content.push_str(&live_session_memory_header_for(&snapshot.session_id));
    content.push_str("\n\n");
    let generated_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let hints = super::schema::live_memory_hints(&generated_at);
    let summary_body = super::schema::normalize_live_summary_markdown(summary, snapshot, &hints);
    content.push_str(&format!(
        "## {} session {}\n\n### Session Stats\n\n- Total tool calls this session: {}\n- Current message count: {}\n\n{}\n",
        generated_at,
        snapshot.session_id,
        snapshot.total_tool_calls,
        snapshot.message_count,
        summary_body
    ));
    truncate_memory_file(content)
}

fn render_entry(
    project_root: &Path,
    session_id: &str,
    report: &CompressionReport,
    files_read: &HashMap<String, usize>,
    files_modified: &[String],
) -> String {
    let generated_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let sections =
        super::schema::structured_sections_from_compaction_summary(report.summary.as_deref());
    let hints = super::schema::compaction_memory_hints(&generated_at);
    let files_read_summary = summarize_read_files(project_root, files_read);
    let files_modified_summary = summarize_modified_files(project_root, files_modified);
    let mut lines = vec![
        format!("## {} session {}", generated_at, session_id),
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

fn read_existing_session_memory(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(err).with_context(|| {
            format!(
                "Failed to read existing session memory file before rewrite: {}",
                path.display()
            )
        }),
    }
}

async fn read_existing_session_memory_async(path: &Path) -> Result<String> {
    match tokio::fs::read_to_string(path).await {
        Ok(content) => Ok(content),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(err).with_context(|| {
            format!(
                "Failed to read existing session memory file before rewrite: {}",
                path.display()
            )
        }),
    }
}

fn load_memory_excerpt(path: &Path, session_id: &str, max_chars: usize) -> Option<String> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(err) => {
            warn!(
                "Failed to read session memory excerpt {}: {err}",
                path.display()
            );
            return None;
        }
    };
    match verify_memory_ownership(&content, session_id) {
        ArtifactOwnership::Exact | ArtifactOwnership::LegacyPrefix => {}
        ArtifactOwnership::Unverifiable => {
            warn!(
                "拒绝恢复会话 {} 的记忆摘要 {}：工件归属无法验证（可能属于其他会话或格式损坏）",
                session_id,
                path.display()
            );
            return None;
        }
    }
    let mut lines = Vec::new();

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty()
            || line == "# Session Memory"
            || line == "# Session Snapshot"
            || line.starts_with("- Session:")
            || line.starts_with("Yode writes this file automatically")
            || line.starts_with("Yode refreshes this file during the session")
        {
            continue;
        }

        let normalized = if line.starts_with('#') || line.starts_with('-') {
            line.to_string()
        } else {
            format!("- {}", line)
        };
        lines.push(normalized);
        if lines.len() >= 10 {
            break;
        }
    }

    if lines.is_empty() {
        return None;
    }

    let mut excerpt = lines.join("\n");
    if excerpt.chars().count() > max_chars {
        excerpt = excerpt.chars().take(max_chars).collect::<String>();
        excerpt.push_str("...");
    }

    Some(excerpt)
}

/// 删除某项目目录下已验证归属 session 的工件（会话删除后的磁盘清理）。
/// 仅删除文件名携带完整 session token 的新工件；绝不触碰旧共享文件或其他会话工件。
pub fn cleanup_session_artifacts(project_root: &Path, session_id: &str) -> Result<CleanupReport> {
    let token = session_artifact_token(session_id);
    let mut report = CleanupReport::default();
    let dirs: &[(&str, &str)] = &[
        ("memory", "会话记忆"),
        ("transcripts", "transcript 工件"),
        ("status", "状态工件"),
        ("turns", "turn 工件"),
        ("tools", "工具工件"),
        ("context-collapse", "上下文压缩工件"),
        ("plans", "计划工件"),
    ];

    for (subdir, label) in dirs {
        let dir = project_root.join(".yode").join(subdir);
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                report.errors.push(format!(
                    "无法读取 {} 目录 {} 进行清理: {}",
                    label,
                    dir.display(),
                    err
                ));
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    report
                        .errors
                        .push(format!("无法枚举 {} 目录条目: {}", dir.display(), err));
                    continue;
                }
            };
            let file_name = entry.file_name().to_string_lossy().to_string();
            let belongs_to_session = if *subdir == "memory" {
                file_name == format!("session-{token}.md")
                    || file_name == format!("session-{token}.live.md")
            } else {
                file_name.starts_with(&format!("{token}-"))
            };
            if !belongs_to_session {
                continue;
            }
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            match remove_verified_artifact_file(&path, session_id) {
                Ok(removed) => {
                    if removed {
                        report.removed_files += 1;
                    }
                }
                Err(err) => {
                    report
                        .errors
                        .push(format!("无法清理 {}: {}", path.display(), err));
                }
            }
        }
    }

    if !report.errors.is_empty() {
        return Err(anyhow::anyhow!(
            "会话 {} 工件清理部分失败（已删除 {} 个文件）:\n{}",
            session_id,
            report.removed_files,
            report.errors.join("\n")
        ));
    }
    Ok(report)
}

/// 删除单个工件前做内容级归属校验：内容携带 session_id 时必须匹配目标会话，
/// 不匹配则拒绝删除并报错；内容不含 session_id 时以文件名 token 为准。
fn remove_verified_artifact_file(path: &Path, session_id: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let content = fs::read_to_string(path).with_context(|| {
        format!(
            "无法读取会话工件 {}，无法验证归属，拒绝删除",
            path.display()
        )
    })?;
    let content_session = json_artifact_session_id(&content)
        .or_else(|| markdown_artifact_session_id(&content))
        .or_else(|| markdown_front_matter_session_id(&content));
    if let Some(owner) = content_session {
        if owner != session_id {
            return Err(anyhow::anyhow!(
                "工件内容归属 session {} 与目标 {} 不一致，拒绝删除",
                owner,
                session_id
            ));
        }
    }
    fs::remove_file(path).with_context(|| {
        format!(
            "无法删除会话工件 {}（删除失败，可能需要手动处理）",
            path.display()
        )
    })?;
    Ok(true)
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CleanupReport {
    pub removed_files: usize,
    pub errors: Vec<String>,
}

#[cfg(test)]
mod legacy_migration_tests {
    use super::*;
    use crate::session_artifact::legacy_session_short_id;

    fn legacy_compaction_memory_body(session_id: &str) -> String {
        format!(
            "{}\n\n## 2026-01-01 12:00:00 session {}\n\n### Goals\n\n- goal\n",
            SESSION_MEMORY_HEADER,
            legacy_session_short_id(session_id)
        )
    }

    #[test]
    fn migrates_legacy_shared_memory_when_fully_attributable() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let session = "12345678-aaaa-bbbb";
        let legacy = root.join(".yode/memory/session.md");
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, legacy_compaction_memory_body(session)).unwrap();

        migrate_legacy_session_memory(root, session).unwrap();

        assert!(!legacy.exists());
        let migrated = session_memory_path(root, session);
        let content = std::fs::read_to_string(&migrated).unwrap();
        assert!(content.contains(session));
        assert_eq!(
            markdown_front_matter_session_id(&content).as_deref(),
            Some(session)
        );
        let excerpt = best_compaction_memory_excerpt(root, session, 2000)
            .map(|(_, e)| e)
            .unwrap();
        assert!(excerpt.contains("### Goals"));
        assert!(excerpt.contains("- goal"));
    }

    #[test]
    fn does_not_migrate_legacy_memory_owned_by_another_session() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let legacy = root.join(".yode/memory/session.md");
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, legacy_compaction_memory_body("87654321-xxxx-yyyy")).unwrap();

        migrate_legacy_session_memory(root, "12345678-aaaa-bbbb").unwrap();

        assert!(legacy.exists());
        assert!(!session_memory_path(root, "12345678-aaaa-bbbb").exists());
        assert!(
            best_compaction_memory_excerpt(root, "12345678-aaaa-bbbb", 2000).is_none(),
            "其他会话的旧共享记忆不得被读取"
        );
    }

    #[test]
    fn does_not_migrate_mixed_session_legacy_memory() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let legacy = root.join(".yode/memory/session.md");
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        let mut body = legacy_compaction_memory_body("12345678-aaaa-bbbb");
        body.push_str("\n\n## 2026-01-02 13:00:00 session 87654321-dddd\n\n### Goals\n\n- other\n");
        std::fs::write(&legacy, body).unwrap();

        migrate_legacy_session_memory(root, "12345678-aaaa-bbbb").unwrap();

        assert!(legacy.exists(), "混合会话旧文件不得删除");
        assert!(!session_memory_path(root, "12345678-aaaa-bbbb").exists());
    }

    #[test]
    fn a_b_session_memory_files_are_isolated() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let report = CompressionReport {
            removed: 2,
            tool_results_truncated: 0,
            summary: Some("A summary".to_string()),
            removed_messages: vec![],
        };
        persist_compaction_memory(root, "session-aaaa-1111", &report, &HashMap::new(), &[])
            .unwrap();
        let report_b = CompressionReport {
            removed: 3,
            tool_results_truncated: 0,
            summary: Some("B summary".to_string()),
            removed_messages: vec![],
        };
        persist_compaction_memory(root, "session-bbbb-2222", &report_b, &HashMap::new(), &[])
            .unwrap();

        let path_a = session_memory_path(root, "session-aaaa-1111");
        let path_b = session_memory_path(root, "session-bbbb-2222");
        assert_ne!(path_a, path_b);
        assert!(path_a.exists());
        assert!(path_b.exists());

        let excerpt_a = best_compaction_memory_excerpt(root, "session-aaaa-1111", 2000).unwrap();
        assert!(excerpt_a.1.contains("A summary"));
        assert!(!excerpt_a.1.contains("B summary"));
        let excerpt_b = best_compaction_memory_excerpt(root, "session-bbbb-2222", 2000).unwrap();
        assert!(excerpt_b.1.contains("B summary"));
        assert!(!excerpt_b.1.contains("A summary"));
    }

    #[test]
    fn deleting_one_session_never_touches_other_sessions() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let a = "session-aaaa-1111";
        let b = "session-bbbb-2222";
        persist_live_session_memory(
            root,
            &build_live_snapshot(a, &[Message::user("A")], 1, &[], &[]),
        )
        .unwrap();
        persist_live_session_memory(
            root,
            &build_live_snapshot(b, &[Message::user("B")], 1, &[], &[]),
        )
        .unwrap();
        std::fs::create_dir_all(root.join(".yode/memory")).unwrap();
        let legacy = root.join(".yode/memory/session.md");
        std::fs::write(&legacy, "# Session Memory\n\n- legacy\n").unwrap();

        cleanup_session_artifacts(root, a).unwrap();

        assert!(!live_session_memory_path(root, a).exists());
        assert!(live_session_memory_path(root, b).exists());
        assert!(legacy.exists(), "旧共享文件绝不能被会话删除误删");
    }

    #[test]
    fn cleanup_rejects_content_mismatch_without_deleting() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let a = "session-aaaa-1111";
        let b = "session-bbbb-2222";
        let status_dir = root.join(".yode/status");
        std::fs::create_dir_all(&status_dir).unwrap();
        let path = status_dir.join(format!(
            "{}-post-compact-restore-state.json",
            session_artifact_token(a)
        ));
        std::fs::write(&path, format!(r#"{{"session_id": "{b}", "blocks": []}}"#)).unwrap();

        let err = cleanup_session_artifacts(root, a).expect_err("内容归属不一致应拒绝删除");
        assert!(path.exists(), "归属不一致的工件不得被删除");
        assert!(err.to_string().contains("不一致"));
    }

    #[test]
    fn cleanup_keeps_unreadable_artifact() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let session = "session-unreadable-1111";
        let status_dir = root.join(".yode/status");
        std::fs::create_dir_all(&status_dir).unwrap();
        let path = status_dir.join(format!(
            "{}-post-compact-restore-state.json",
            session_artifact_token(session)
        ));
        // 无效 UTF-8 使 read_to_string 明确失败，不能被静默当作无归属文件删除。
        std::fs::write(&path, [0xff, 0xfe, 0xfd]).unwrap();

        let err = cleanup_session_artifacts(root, session).expect_err("不可读工件应拒绝删除");
        assert!(path.exists());
        assert!(err.to_string().contains("无法读取会话工件"));
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_keeps_permission_denied_artifact() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let session = "session-permission-1111";
        let status_dir = root.join(".yode/status");
        std::fs::create_dir_all(&status_dir).unwrap();
        let path = status_dir.join(format!(
            "{}-post-compact-restore-state.json",
            session_artifact_token(session)
        ));
        std::fs::write(&path, b"{}\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        // root 用户可能绕过权限位，无法在该环境稳定注入 EACCES 时跳过。
        if std::fs::read_to_string(&path).is_ok() {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            return;
        }

        let err = cleanup_session_artifacts(root, session).expect_err("权限异常应拒绝删除");
        assert!(path.exists());
        assert!(err.to_string().contains("无法读取会话工件"));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}
