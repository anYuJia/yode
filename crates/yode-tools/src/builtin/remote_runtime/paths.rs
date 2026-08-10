use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Deserialize;

use crate::session_artifact::{
    file_mentions_legacy_short_id, file_mentions_session_token, json_artifact_session_id,
    legacy_rename_target_file_name, legacy_session_short_id, markdown_artifact_session_id,
    rename_verified_legacy_artifact, session_artifact_token,
};

pub(super) async fn latest_artifact_by_suffix_async(dir: &Path, suffix: &str) -> Option<PathBuf> {
    let mut entries = tokio::fs::read_dir(dir).await.ok()?;
    let mut paths = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(suffix))
        {
            paths.push(path);
        }
    }
    paths.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
    paths.into_iter().next()
}

/// 收集目录下属于目标会话的工件候选（新命名携带完整 token 或旧版携带短 ID，
/// 短横线分隔组件、任意位置），按文件名倒序（最新优先）。
async fn session_artifact_candidates(dir: &Path, suffix: &str, session_id: &str) -> Vec<PathBuf> {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut paths = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !file_name.ends_with(suffix) {
            continue;
        }
        if file_mentions_session_token(file_name, session_id)
            || file_mentions_legacy_short_id(file_name, session_id)
        {
            paths.push(path);
        }
    }
    paths.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
    paths
}

/// 最新优先加载属于目标会话的工件：
/// - 内容携带 session id 时做完整校验，不匹配的候选（包括其他会话的旧短 ID 工件）跳过并记录诊断；
/// - 内容无法验证归属的候选拒绝恢复；
/// - 旧版短 ID 工件验证通过后一次性改名迁移到新命名。
pub(super) async fn latest_session_artifact_by_suffix_async(
    dir: &Path,
    suffix: &str,
    session_id: &str,
) -> Option<PathBuf> {
    for candidate in session_artifact_candidates(dir, suffix, session_id).await {
        let content = match tokio::fs::read_to_string(&candidate).await {
            Ok(content) => content,
            Err(err) => {
                tracing::warn!("无法读取工件 {}: {}", candidate.display(), err);
                continue;
            }
        };
        match verify_session_artifact_content(&content, session_id) {
            Verified::Match => {
                let file_name = candidate
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                if let Some(new_name) = legacy_rename_target_file_name(&file_name, session_id) {
                    let token_path = candidate.with_file_name(new_name);
                    rename_verified_legacy_artifact(&candidate, &token_path, session_id);
                    if token_path.exists() {
                        return Some(token_path);
                    }
                }
                return Some(candidate);
            }
            Verified::Rejected(reason) => {
                tracing::warn!(
                    "拒绝恢复会话 {} 的工件 {}：{}",
                    session_id,
                    candidate.display(),
                    reason
                );
            }
        }
    }
    None
}

enum Verified {
    Match,
    Rejected(String),
}

fn verify_session_artifact_content(content: &str, session_id: &str) -> Verified {
    if let Some(owner) = json_artifact_session_id(content)
        .or_else(|| markdown_artifact_session_id(content))
        .or_else(|| jsonl_artifact_session_id(content))
    {
        if owner == session_id {
            Verified::Match
        } else {
            Verified::Rejected(format!("内容归属 session {} 与目标不一致", owner))
        }
    } else {
        Verified::Rejected("缺少可验证的 Session 标识".to_string())
    }
}

/// JSONL 工件（如事件日志）取最后一条有效记录校验归属。
fn jsonl_artifact_session_id(content: &str) -> Option<String> {
    content.lines().rev().find_map(|line| {
        serde_json::from_str::<serde_json::Value>(line.trim())
            .ok()
            .and_then(|value| {
                value
                    .get("session_id")?
                    .as_str()
                    .map(str::trim)
                    .map(str::to_string)
            })
    })
}

pub(super) async fn latest_remote_control_state_artifact_async(
    project_root: &Path,
    session_id: &str,
) -> Option<PathBuf> {
    latest_session_artifact_by_suffix_async(
        &remote_dir(project_root),
        "remote-control-session.json",
        session_id,
    )
    .await
}

pub(super) async fn latest_remote_transport_state_artifact_async(
    project_root: &Path,
    session_id: &str,
) -> Option<PathBuf> {
    latest_session_artifact_by_suffix_async(
        &remote_dir(project_root),
        "remote-transport-state.json",
        session_id,
    )
    .await
}

pub(super) async fn latest_remote_transport_events_artifact_async(
    project_root: &Path,
    session_id: &str,
) -> Option<PathBuf> {
    latest_session_artifact_by_suffix_async(
        &remote_dir(project_root),
        "remote-transport-events.md",
        session_id,
    )
    .await
}

#[cfg(test)]
pub(super) async fn latest_remote_transport_event_log_artifact_async(
    project_root: &Path,
    session_id: &str,
) -> Option<PathBuf> {
    latest_session_artifact_by_suffix_async(
        &remote_dir(project_root),
        "remote-events.jsonl",
        session_id,
    )
    .await
}

pub(super) async fn latest_remote_live_session_state_artifact_async(
    project_root: &Path,
    session_id: &str,
) -> Option<PathBuf> {
    latest_session_artifact_by_suffix_async(
        &remote_dir(project_root),
        "remote-live-session-state.json",
        session_id,
    )
    .await
}

/// 判断 transcript 文件名是否属于目标会话（新命名 `{token}-compact-` 或旧版短 ID 命名）。
pub(super) fn transcript_file_candidate(file_name: &str, session_id: &str) -> bool {
    let token = session_artifact_token(session_id);
    if file_name.starts_with(&format!("{token}-compact-")) {
        return true;
    }
    file_name.starts_with(&format!("{}-compact-", legacy_session_short_id(session_id)))
}

/// 当前会话最新的 transcript 工件路径（只恢复内容验证归属本会话的工件）。
pub(super) async fn latest_transcript_artifact_async(
    project_root: &Path,
    session_id: &str,
) -> Option<String> {
    let dir = project_root.join(".yode").join("transcripts");
    let mut entries = tokio::fs::read_dir(&dir).await.ok()?;
    let mut paths = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if file_name.ends_with(".md") && transcript_file_candidate(file_name, session_id) {
            paths.push(path);
        }
    }
    paths.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
    for candidate in paths {
        let content = match tokio::fs::read_to_string(&candidate).await {
            Ok(content) => content,
            Err(err) => {
                tracing::warn!("无法读取 transcript 工件 {}: {}", candidate.display(), err);
                continue;
            }
        };
        match markdown_artifact_session_id(&content) {
            Some(owner) if owner == session_id => {
                return Some(candidate.display().to_string());
            }
            Some(owner) => {
                tracing::warn!(
                    "拒绝恢复会话 {} 的 transcript 工件 {}：内容归属 session {}",
                    session_id,
                    candidate.display(),
                    owner
                );
            }
            None => {
                tracing::warn!(
                    "拒绝恢复会话 {} 的 transcript 工件 {}：缺少可验证的 Session 标识",
                    session_id,
                    candidate.display()
                );
            }
        }
    }
    None
}

pub(super) async fn load_json_async<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    Ok(serde_json::from_str(
        &tokio::fs::read_to_string(path).await?,
    )?)
}

pub(super) fn timestamp_slug() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

pub(super) fn now_string() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub(super) fn remote_dir(project_root: &Path) -> PathBuf {
    project_root.join(".yode").join("remote")
}

/// 当前会话的远端事件日志路径（按完整 session id 隔离）。
pub(super) fn remote_transport_event_log_path(project_root: &Path, session_id: &str) -> PathBuf {
    remote_dir(project_root).join(format!(
        "{}-remote-events.jsonl",
        session_artifact_token(session_id)
    ))
}

/// 兼容查找旧版短 ID 事件日志（仅用于一次性迁移）。
fn legacy_remote_transport_event_log_path(project_root: &Path, session_id: &str) -> PathBuf {
    remote_dir(project_root).join(format!(
        "{}-remote-events.jsonl",
        legacy_session_short_id(session_id)
    ))
}

/// 读取当前会话事件日志的游标；旧版短 ID 日志仅当最后一行归属当前会话时迁移并读取。
pub(super) async fn read_remote_event_log_cursor_async(
    project_root: &Path,
    session_id: &str,
) -> Option<u64> {
    let path = remote_transport_event_log_path(project_root, session_id);
    let body = match tokio::fs::read_to_string(&path).await {
        Ok(body) => body,
        Err(_) => {
            let legacy = legacy_remote_transport_event_log_path(project_root, session_id);
            let body = tokio::fs::read_to_string(&legacy).await.ok()?;
            let last_session = body.lines().rev().find_map(|line| {
                serde_json::from_str::<serde_json::Value>(line.trim())
                    .ok()
                    .and_then(|value| value.get("session_id")?.as_str().map(str::to_string))
            });
            if last_session.as_deref() != Some(session_id) {
                return None;
            }
            rename_verified_legacy_artifact(&legacy, &path, session_id);
            body
        }
    };
    remote_event_log_cursor_from_body(&body)
}

pub(super) fn remote_event_log_cursor_from_body(body: &str) -> Option<u64> {
    body.lines().rev().find_map(|line| {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        serde_json::from_str::<serde_json::Value>(line)
            .ok()
            .and_then(|value| value.get("cursor").and_then(|cursor| cursor.as_u64()))
    })
}
