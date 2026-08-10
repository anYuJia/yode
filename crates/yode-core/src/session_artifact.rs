use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

/// 会话工件命名的确定性安全 token：
/// - session id 本身路径安全（仅字母数字、`-`、`_`、`.`，不以 `.` 开头且不含 `..`）时原样使用，
///   保证可读、确定性、抗碰撞；
/// - 否则回退为 session id 的 SHA-256 十六进制前 16 位（64 位熵），同样确定性且抗碰撞。
pub fn session_artifact_token(session_id: &str) -> String {
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        return "empty".to_string();
    }
    let safe = trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        && !trimmed.starts_with('.')
        && !trimmed.contains("..");
    if safe {
        trimmed.to_string()
    } else {
        let mut hasher = Sha256::new();
        hasher.update(trimmed.as_bytes());
        let digest = hasher.finalize();
        digest
            .iter()
            .take(8)
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

/// 旧版工件使用的会话短 ID（前 8 位），仅用于兼容性查找与校验，不再用于新工件命名。
pub fn legacy_session_short_id(session_id: &str) -> String {
    session_id.chars().take(8).collect()
}

/// 文件名是否属于当前会话的新命名工件（`{token}-...`）。
pub fn file_belongs_to_session(file_name: &str, session_id: &str) -> bool {
    let token = session_artifact_token(session_id);
    file_name.strip_prefix(&format!("{token}-")).is_some()
}

/// 文件名是否为当前会话的旧版短 ID 工件（`{short8}-...`）。
pub fn file_matches_legacy_short_id(file_name: &str, session_id: &str) -> bool {
    let short = legacy_session_short_id(session_id);
    file_name.strip_prefix(&format!("{short}-")).is_some()
}

/// 从工件内容中解析会话归属。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactOwnership {
    /// 内容明确携带完整 session id 且与目标一致。
    Exact,
    /// 内容携带 8 位旧版短 id，且与目标 session 前缀一致。
    LegacyPrefix,
    /// 无法验证归属（缺失、损坏、属于其他会话）。
    Unverifiable,
}

fn ids_match_fully(content_id: &str, session_id: &str) -> bool {
    content_id == session_id
}

/// 校验内存文件归属：优先 `- Session: {full}` 头行，其次 `## ... session {id}` 条目标记。
/// 旧版共享内存仅当全部条目都归属同一会话（完整或 8 位前缀匹配）时才视为可迁移。
pub fn verify_memory_ownership(content: &str, session_id: &str) -> ArtifactOwnership {
    if let Some(front_matter_id) = markdown_front_matter_session_id(content) {
        return if ids_match_fully(&front_matter_id, session_id) {
            ArtifactOwnership::Exact
        } else {
            ArtifactOwnership::Unverifiable
        };
    }

    let entry_ids = markdown_entry_session_ids(content);
    if entry_ids.is_empty() {
        return ArtifactOwnership::Unverifiable;
    }

    let short = legacy_session_short_id(session_id);
    let mut any_legacy = false;
    for entry_id in &entry_ids {
        if ids_match_fully(entry_id, session_id) {
            continue;
        }
        if entry_id.len() <= 8 && entry_id == &short {
            any_legacy = true;
            continue;
        }
        return ArtifactOwnership::Unverifiable;
    }

    if entry_ids.iter().all(|id| ids_match_fully(id, session_id)) {
        ArtifactOwnership::Exact
    } else if any_legacy {
        ArtifactOwnership::LegacyPrefix
    } else {
        ArtifactOwnership::Unverifiable
    }
}

/// 从内存文件内容中解析 `- Session: {id}` 头行（新格式 front-matter）。
pub fn markdown_front_matter_session_id(content: &str) -> Option<String> {
    content.lines().take(6).find_map(|line| {
        let value = line.trim().strip_prefix("- Session: ")?;
        (!value.trim().is_empty()).then(|| value.trim().to_string())
    })
}

/// 从内存文件内容中解析所有 `## ... session {id}` 条目标记（新旧格式通用）。
pub fn markdown_entry_session_ids(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if !line.starts_with("## ") {
                return None;
            }
            let id = line.split(" session ").last()?.trim();
            if id.is_empty() || id.contains(' ') || id.contains('\t') {
                return None;
            }
            Some(id.to_string())
        })
        .collect()
}

/// 从 JSON 工件内容中解析 `session_id` 字段。
pub fn json_artifact_session_id(content: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(content)
        .ok()?
        .get("session_id")?
        .as_str()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

/// 从 Markdown 工件内容中解析 `- Session: {id}` 行。
pub fn markdown_artifact_session_id(content: &str) -> Option<String> {
    content.lines().take(24).find_map(|line| {
        let value = line.trim().strip_prefix("- Session: ")?;
        (!value.trim().is_empty()).then(|| value.trim().to_string())
    })
}

/// 将已验证归属的旧版短 ID 工件重命名为新命名（一次性迁移）。
/// 调用方必须已通过内容校验确认旧文件归属目标 session；
/// 重命名失败仅记录诊断，不影响已读取的内容。
pub fn rename_verified_legacy_artifact(legacy: &Path, target: &Path, session_id: &str) {
    if !legacy.exists() || target.exists() {
        return;
    }
    match std::fs::rename(legacy, target) {
        Ok(()) => {
            tracing::info!(
                "已迁移旧版短 ID 工件 {} 到 {}（归属 session {}）",
                legacy.display(),
                target.display(),
                session_id
            );
        }
        Err(err) => {
            tracing::warn!(
                "旧版短 ID 工件迁移失败 {} -> {}: {}（不影响本次读取）",
                legacy.display(),
                target.display(),
                err
            );
        }
    }
}

const ATOMIC_WRITE_RETRIES: usize = 3;

/// 原子替换写入：先写同目录临时文件再 rename，避免并发会话读到半截内容；
/// 带重试，失败时清理临时文件。
pub fn atomic_write_sync(path: &Path, content: &str) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("无法确定工件父目录: {}", path.display()))?;
    let mut last_error = None;
    for attempt in 0..ATOMIC_WRITE_RETRIES {
        let temp = parent.join(format!(
            ".{}.yode-{}.tmp",
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "artifact".to_string()),
            uuid::Uuid::new_v4()
        ));
        let result = (|| -> Result<()> {
            fs::write(&temp, content)?;
            fs::rename(&temp, path)?;
            Ok(())
        })();
        match result {
            Ok(()) => return Ok(()),
            Err(err) => {
                let _ = fs::remove_file(&temp);
                last_error = Some(err);
                if attempt + 1 < ATOMIC_WRITE_RETRIES {
                    std::thread::sleep(std::time::Duration::from_millis(25 * (attempt as u64 + 1)));
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("工件原子写入失败但未产生 I/O 错误")))
}

/// 原子替换写入的异步版本。
pub async fn atomic_write_async(path: &Path, content: &str) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("无法确定工件父目录: {}", path.display()))?;
    let mut last_error = None;
    for attempt in 0..ATOMIC_WRITE_RETRIES {
        let temp = parent.join(format!(
            ".{}.yode-{}.tmp",
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "artifact".to_string()),
            uuid::Uuid::new_v4()
        ));
        let result = async {
            tokio::fs::write(&temp, content).await?;
            tokio::fs::rename(&temp, path).await?;
            Ok::<(), anyhow::Error>(())
        }
        .await;
        match result {
            Ok(()) => return Ok(()),
            Err(err) => {
                let _ = tokio::fs::remove_file(&temp).await;
                last_error = Some(err);
                if attempt + 1 < ATOMIC_WRITE_RETRIES {
                    tokio::time::sleep(std::time::Duration::from_millis(25 * (attempt as u64 + 1)))
                        .await;
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("工件原子写入失败但未产生 I/O 错误")))
}

/// 为会话工件目录做路径安全检查：拒绝把文件写进项目根之外的路径。
pub fn session_artifact_path(
    project_root: &Path,
    subdirs: &[&str],
    session_id: &str,
    suffix: &str,
) -> PathBuf {
    let mut path = project_root.join(".yode");
    for subdir in subdirs {
        path = path.join(subdir);
    }
    path.join(format!("{}-{}", session_artifact_token(session_id), suffix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_session_ids_are_used_verbatim() {
        assert_eq!(session_artifact_token("abc-123"), "abc-123");
        assert_eq!(
            session_artifact_token("12345678-1234-1234-1234-123456789012"),
            "12345678-1234-1234-1234-123456789012"
        );
    }

    #[test]
    fn unsafe_session_ids_fall_back_to_sha256() {
        let token = session_artifact_token("../evil/../path");
        assert_eq!(token.len(), 16);
        assert!(!token.contains('/'));
        assert_eq!(session_artifact_token("../evil/../path"), token);
        assert_eq!(session_artifact_token(""), "empty");
    }

    #[test]
    fn verify_memory_ownership_matches_full_ids() {
        let content =
            "# Session Memory\n\n- Session: session-full-1\n\n## 2026-01-01 session-full-1\n";
        assert_eq!(
            verify_memory_ownership(content, "session-full-1"),
            ArtifactOwnership::Exact
        );
        assert_eq!(
            verify_memory_ownership(content, "session-full-2"),
            ArtifactOwnership::Unverifiable
        );
    }

    #[test]
    fn verify_memory_ownership_accepts_single_session_legacy_prefix() {
        let content = "# Session Memory\n\n## 2026-01-01 session 12345678\n";
        assert_eq!(
            verify_memory_ownership(content, "12345678-aaaa-bbbb"),
            ArtifactOwnership::LegacyPrefix
        );
        let content_full = "# Session Memory\n\n## 2026-01-01 session 12345678-aaaa-bbbb\n";
        assert_eq!(
            verify_memory_ownership(content_full, "12345678-aaaa-bbbb"),
            ArtifactOwnership::Exact
        );
        let mixed =
            "# Session Memory\n\n## 2026-01-01 session 12345678\n## 2026-01-02 session 87654321\n";
        assert_eq!(
            verify_memory_ownership(mixed, "12345678-aaaa-bbbb"),
            ArtifactOwnership::Unverifiable
        );
    }

    #[test]
    fn json_and_markdown_session_id_parsers() {
        assert_eq!(
            json_artifact_session_id(r#"{"session_id":"s-1","mode":"auto"}"#),
            Some("s-1".to_string())
        );
        assert_eq!(json_artifact_session_id(r#"{"mode":"auto"}"#), None);
        assert_eq!(
            markdown_artifact_session_id("# T\n\n- Session: s-2\n- Mode: x\n"),
            Some("s-2".to_string())
        );
        assert_eq!(markdown_artifact_session_id("# T\n\n- Mode: x\n"), None);
    }

    #[test]
    fn atomic_write_replaces_and_cleans_temp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.json");
        atomic_write_sync(&path, "one").unwrap();
        atomic_write_sync(&path, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn file_belongs_to_session_matches_token_only() {
        assert!(file_belongs_to_session(
            "12345678-aaaa-bbbb-latest-turn.json",
            "12345678-aaaa-bbbb"
        ));
        assert!(!file_belongs_to_session(
            "12345678-aaaa-latest-turn.json",
            "12345678-aaaa-bbbb"
        ));
        assert!(file_matches_legacy_short_id(
            "12345678-latest-turn.json",
            "12345678-aaaa-bbbb"
        ));
        assert!(!file_matches_legacy_short_id(
            "87654321-latest-turn.json",
            "12345678-aaaa-bbbb"
        ));
    }
}
