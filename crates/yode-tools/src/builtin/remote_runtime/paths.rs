use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Deserialize;

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

pub(super) async fn latest_remote_control_state_artifact_async(
    project_root: &Path,
) -> Option<PathBuf> {
    latest_artifact_by_suffix_async(&remote_dir(project_root), "remote-control-session.json").await
}

pub(super) async fn latest_remote_transport_state_artifact_async(
    project_root: &Path,
) -> Option<PathBuf> {
    latest_artifact_by_suffix_async(&remote_dir(project_root), "remote-transport-state.json").await
}

pub(super) async fn latest_remote_transport_events_artifact_async(
    project_root: &Path,
) -> Option<PathBuf> {
    latest_artifact_by_suffix_async(&remote_dir(project_root), "remote-transport-events.md").await
}

#[cfg(test)]
pub(super) async fn latest_remote_transport_event_log_artifact_async(
    project_root: &Path,
) -> Option<PathBuf> {
    latest_artifact_by_suffix_async(&remote_dir(project_root), "remote-events.jsonl").await
}

pub(super) async fn latest_remote_live_session_state_artifact_async(
    project_root: &Path,
) -> Option<PathBuf> {
    latest_artifact_by_suffix_async(&remote_dir(project_root), "remote-live-session-state.json")
        .await
}

pub(super) async fn latest_transcript_artifact_async(project_root: &Path) -> Option<String> {
    latest_artifact_by_suffix_async(&project_root.join(".yode").join("transcripts"), ".md")
        .await
        .map(|path| path.display().to_string())
}

pub(super) async fn load_json_async<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    Ok(serde_json::from_str(
        &tokio::fs::read_to_string(path).await?,
    )?)
}

pub(super) fn short_session(session_id: &str) -> String {
    session_id.chars().take(8).collect()
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

pub(super) fn remote_transport_event_log_path(project_root: &Path, session_id: &str) -> PathBuf {
    remote_dir(project_root).join(format!("{}-remote-events.jsonl", short_session(session_id)))
}

pub(super) async fn read_remote_event_log_cursor_async(path: &Path) -> Option<u64> {
    let body = tokio::fs::read_to_string(path).await.ok()?;
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
