use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Deserialize;

pub(super) fn latest_artifact_by_suffix(dir: &Path, suffix: &str) -> Option<PathBuf> {
    let mut entries = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(suffix))
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
    entries.into_iter().next()
}

pub(super) fn latest_remote_control_state_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(&remote_dir(project_root), "remote-control-session.json")
}

pub(super) fn latest_remote_transport_state_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(&remote_dir(project_root), "remote-transport-state.json")
}

pub(super) fn latest_remote_transport_events_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(&remote_dir(project_root), "remote-transport-events.md")
}

#[cfg(test)]
pub(super) fn latest_remote_transport_event_log_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(&remote_dir(project_root), "remote-events.jsonl")
}

pub(super) fn latest_remote_live_session_state_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(&remote_dir(project_root), "remote-live-session-state.json")
}

pub(super) fn latest_transcript_artifact(project_root: &Path) -> Option<String> {
    latest_artifact_by_suffix(&project_root.join(".yode").join("transcripts"), ".md")
        .map(|path| path.display().to_string())
}

pub(super) fn load_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
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

pub(super) fn read_remote_event_log_cursor(path: &Path) -> Option<u64> {
    let body = std::fs::read_to_string(path).ok()?;
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
