mod render;
mod summary;
#[cfg(test)]
mod tests;
mod writer;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use yode_llm::types::Message;

use crate::context_manager::CompressionReport;
use crate::engine::CompactBoundaryRuntimeState;
use crate::session_artifact::{atomic_write_async, session_artifact_token};

const TRANSCRIPTS_DIR: &str = ".yode/transcripts";

/// 判断 transcript 文件名是否属于目标会话：
/// 新命名 `{token}-compact-{ts}.md`，或旧版短 ID `{short8}-compact-{ts}.md`（兼容查找用）。
pub(crate) fn transcript_file_candidate(file_name: &str, session_id: &str) -> bool {
    let token = session_artifact_token(session_id);
    if file_name.starts_with(&format!("{token}-compact-")) {
        return true;
    }
    let short = crate::session_artifact::legacy_session_short_id(session_id);
    file_name.starts_with(&format!("{short}-compact-"))
}

/// 一次性迁移旧版短 ID transcript 文件名到新命名，并返回当前可用路径。
/// 调用方必须先验证内容归属目标 session；重命名失败仅记录诊断，不影响读取。
pub(crate) fn migrate_legacy_transcript_file(path: &Path, session_id: &str) -> PathBuf {
    use crate::session_artifact::legacy_session_short_id;
    use tracing::{info, warn};

    let Some(file_name) = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
    else {
        return path.to_path_buf();
    };
    let token = session_artifact_token(session_id);
    if file_name.starts_with(&format!("{token}-compact-")) {
        return path.to_path_buf();
    }
    let short = legacy_session_short_id(session_id);
    let Some(rest) = file_name.strip_prefix(&format!("{short}-compact-")) else {
        return path.to_path_buf();
    };
    let new_path = path.with_file_name(format!("{token}-compact-{rest}"));
    if new_path.exists() {
        return new_path;
    }
    match std::fs::rename(path, &new_path) {
        Ok(()) => {
            info!(
                "已迁移旧版 transcript 文件名 {} 到 {}",
                path.display(),
                new_path.display()
            );
            new_path
        }
        Err(err) => {
            warn!(
                "旧版 transcript 迁移失败 {} -> {}: {}（不影响本次读取）",
                path.display(),
                new_path.display(),
                err
            );
            path.to_path_buf()
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "transcript writer passes the full compaction context through to the renderer"
)]
pub fn write_compaction_transcript(
    project_root: &Path,
    session_id: &str,
    messages: &[Message],
    report: &CompressionReport,
    mode: &str,
    failed_tool_call_ids: &HashSet<String>,
    session_memory_path: Option<&Path>,
    files_read: &HashMap<String, usize>,
    files_modified: &[String],
    compact_boundary: Option<&CompactBoundaryRuntimeState>,
) -> Result<PathBuf> {
    let dir = project_root.join(TRANSCRIPTS_DIR);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create transcript dir: {}", dir.display()))?;

    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!(
        "{}-compact-{}.md",
        session_artifact_token(session_id),
        timestamp
    ));
    let compact_boundary = compact_boundary.cloned().map(|mut boundary| {
        if !boundary
            .artifact_paths
            .iter()
            .any(|artifact| artifact == &path.display().to_string())
        {
            boundary.artifact_paths.push(path.display().to_string());
        }
        boundary
    });

    writer::write_string_with_retry(
        &path,
        &render::render_compaction_transcript(
            project_root,
            session_id,
            messages,
            report,
            mode,
            failed_tool_call_ids,
            session_memory_path,
            files_read,
            files_modified,
            compact_boundary.as_ref(),
        ),
    )
    .with_context(|| format!("Failed to write transcript file: {}", path.display()))?;

    Ok(path)
}

#[expect(
    clippy::too_many_arguments,
    reason = "transcript writer passes the full compaction context through to the renderer"
)]
pub async fn write_compaction_transcript_async(
    project_root: &Path,
    session_id: &str,
    messages: &[Message],
    report: &CompressionReport,
    mode: &str,
    failed_tool_call_ids: &HashSet<String>,
    session_memory_path: Option<&Path>,
    files_read: &HashMap<String, usize>,
    files_modified: &[String],
    compact_boundary: Option<&CompactBoundaryRuntimeState>,
) -> Result<PathBuf> {
    let dir = project_root.join(TRANSCRIPTS_DIR);
    tokio::fs::create_dir_all(&dir)
        .await
        .with_context(|| format!("Failed to create transcript dir: {}", dir.display()))?;

    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!(
        "{}-compact-{}.md",
        session_artifact_token(session_id),
        timestamp
    ));
    let compact_boundary = compact_boundary.cloned().map(|mut boundary| {
        if !boundary
            .artifact_paths
            .iter()
            .any(|artifact| artifact == &path.display().to_string())
        {
            boundary.artifact_paths.push(path.display().to_string());
        }
        boundary
    });

    atomic_write_async(
        &path,
        &render::render_compaction_transcript(
            project_root,
            session_id,
            messages,
            report,
            mode,
            failed_tool_call_ids,
            session_memory_path,
            files_read,
            files_modified,
            compact_boundary.as_ref(),
        ),
    )
    .await
    .with_context(|| format!("Failed to write transcript file: {}", path.display()))?;

    Ok(path)
}
