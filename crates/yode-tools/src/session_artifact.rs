use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

/// 会话工件命名的确定性安全 token。
///
/// 与 yode-core 的 `crate::session_artifact` 保持同步：
/// yode-tools 不能依赖 yode-core，因此此处维护一份最小等价实现，
/// 修改时必须在两侧同步更新。
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

/// 旧版工件使用的会话短 ID（前 8 位），仅用于兼容性查找与校验。
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

/// 文件名是否以短横线分隔的组件形式携带完整 session token（任意位置，如 `{stamp}-{token}-...`）。
pub fn file_mentions_session_token(file_name: &str, session_id: &str) -> bool {
    let token = session_artifact_token(session_id);
    file_name == token
        || file_name.starts_with(&format!("{token}-"))
        || file_name.ends_with(&format!("-{token}"))
        || file_name.contains(&format!("-{token}-"))
}

/// 文件名是否以短横线分隔的组件形式携带旧版短 ID（任意位置）。
pub fn file_mentions_legacy_short_id(file_name: &str, session_id: &str) -> bool {
    let short = legacy_session_short_id(session_id);
    file_name == short
        || file_name.starts_with(&format!("{short}-"))
        || file_name.ends_with(&format!("-{short}"))
        || file_name.contains(&format!("-{short}-"))
}

/// 将旧版短 ID 工件文件名中的短 ID 组件替换为完整 token（任意位置），
/// 生成一次性迁移后的新文件名；不匹配时返回 None。
pub fn legacy_rename_target_file_name(file_name: &str, session_id: &str) -> Option<String> {
    let short = legacy_session_short_id(session_id);
    let token = session_artifact_token(session_id);
    if let Some(rest) = file_name.strip_prefix(&format!("{short}-")) {
        return Some(format!("{token}-{rest}"));
    }
    if let Some(index) = file_name.find(&format!("-{short}-")) {
        let mut target = String::with_capacity(file_name.len() + token.len() - short.len());
        target.push_str(&file_name[..index + 1]);
        target.push_str(&token);
        target.push_str(&file_name[index + 1 + short.len()..]);
        return Some(target);
    }
    None
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

/// 原子替换写入（临时文件 + rename），带重试，失败时清理临时文件。
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
            std::fs::write(&temp, content)?;
            std::fs::rename(&temp, path)?;
            Ok(())
        })();
        match result {
            Ok(()) => return Ok(()),
            Err(err) => {
                let _ = std::fs::remove_file(&temp);
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

/// 会话工件路径（`{token}-{suffix}`）。
pub fn session_artifact_path(
    project_root: &Path,
    subdir: &str,
    session_id: &str,
    suffix: &str,
) -> PathBuf {
    project_root.join(".yode").join(subdir).join(format!(
        "{}-{}",
        session_artifact_token(session_id),
        suffix
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_deterministic_and_collision_resistant() {
        assert_eq!(
            session_artifact_token("session-12345678"),
            "session-12345678"
        );
        let hashed = session_artifact_token("../evil");
        assert_eq!(hashed.len(), 16);
        assert!(!hashed.contains('/'));
        assert_eq!(hashed, session_artifact_token("../evil"));
        assert_eq!(session_artifact_token(""), "empty");
    }

    #[test]
    fn file_matching_differentiates_full_and_short_tokens() {
        assert!(file_belongs_to_session(
            "12345678-aaaa-bbbb-remote-control.md",
            "12345678-aaaa-bbbb"
        ));
        assert!(!file_belongs_to_session(
            "12345678-aaaa-remote-control.md",
            "12345678-aaaa-bbbb"
        ));
        assert!(file_matches_legacy_short_id(
            "12345678-remote-control.md",
            "12345678-aaaa-bbbb"
        ));
    }

    #[test]
    fn json_and_markdown_session_id_parsers() {
        assert_eq!(
            json_artifact_session_id(r#"{"session_id":"s-1","x":1}"#),
            Some("s-1".to_string())
        );
        assert_eq!(json_artifact_session_id(r#"{"x":1}"#), None);
        assert_eq!(
            markdown_artifact_session_id("# T\n\n- Session: s-2\n"),
            Some("s-2".to_string())
        );
    }

    #[test]
    fn atomic_write_replaces_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.json");
        atomic_write_sync(&path, "one").unwrap();
        atomic_write_sync(&path, "two").unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "two");
    }
}
