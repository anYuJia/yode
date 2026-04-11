use super::store::retention_from_env;
use super::{
    latest_transcript_artifact_path, RuntimeTaskNotificationSeverity, RuntimeTaskStatus,
    RuntimeTaskStore,
};

#[test]
fn runtime_task_store_tracks_lifecycle_and_notifications() {
    let mut store = RuntimeTaskStore::new();
    let (task, _cancel_rx) = store.create(
        "bash".to_string(),
        "bash".to_string(),
        "background build".to_string(),
        "/tmp/task.log".to_string(),
    );

    assert_eq!(task.status, RuntimeTaskStatus::Pending);
    store.mark_running(&task.id);
    store.update_progress(&task.id, "running".to_string());
    store.mark_completed(&task.id);

    let snapshot = store.get(&task.id).unwrap();
    assert_eq!(snapshot.status, RuntimeTaskStatus::Completed);
    assert_eq!(snapshot.attempt, 1);
    assert_eq!(snapshot.last_progress.as_deref(), Some("running"));
    assert!(snapshot.last_progress_at.is_some());
    assert_eq!(snapshot.progress_history, vec!["running".to_string()]);

    let notifications = store.drain_notifications();
    assert_eq!(notifications.len(), 1);
    assert_eq!(
        notifications[0].severity,
        RuntimeTaskNotificationSeverity::Success
    );
    assert!(notifications[0].message.contains("completed"));
}

#[test]
fn runtime_task_store_keeps_bounded_progress_history() {
    let mut store = RuntimeTaskStore::new();
    let (task, _cancel_rx) = store.create(
        "bash".to_string(),
        "bash".to_string(),
        "background build".to_string(),
        "/tmp/task.log".to_string(),
    );

    for i in 0..12 {
        store.update_progress(&task.id, format!("line {}", i));
    }
    store.update_progress(&task.id, "line 11".to_string());
    store.update_progress(&task.id, "".to_string());

    let snapshot = store.get(&task.id).unwrap();
    assert_eq!(snapshot.last_progress.as_deref(), Some("line 11"));
    assert!(snapshot.last_progress_at.is_some());
    assert_eq!(snapshot.progress_history.len(), 8);
    assert_eq!(
        snapshot.progress_history.first().map(String::as_str),
        Some("line 4")
    );
    assert_eq!(
        snapshot.progress_history.last().map(String::as_str),
        Some("line 11")
    );
}

#[test]
fn runtime_task_store_tracks_retry_lineage() {
    let mut store = RuntimeTaskStore::new();
    let (first, _cancel_rx) = store.create(
        "bash".to_string(),
        "bash".to_string(),
        "background build".to_string(),
        "/tmp/task-1.log".to_string(),
    );
    store.mark_failed(&first.id, "boom".to_string());

    let (retry, _cancel_rx) = store.create(
        "bash".to_string(),
        "bash".to_string(),
        "background build".to_string(),
        "/tmp/task-2.log".to_string(),
    );

    assert_eq!(retry.attempt, 2);
    assert_eq!(retry.retry_of.as_deref(), Some(first.id.as_str()));
}

#[test]
fn runtime_task_store_keeps_transcript_backlink() {
    let mut store = RuntimeTaskStore::new();
    let (task, _cancel_rx) = store.create_with_transcript(
        "agent".to_string(),
        "agent".to_string(),
        "background review".to_string(),
        "/tmp/task.log".to_string(),
        Some("/tmp/transcript.md".to_string()),
    );

    let snapshot = store.get(&task.id).unwrap();
    assert_eq!(
        snapshot.transcript_path.as_deref(),
        Some("/tmp/transcript.md")
    );
}

#[test]
fn latest_transcript_artifact_prefers_newest_compaction_timestamp() {
    let dir = tempfile::tempdir().unwrap();
    let transcript_dir = dir.path().join(".yode").join("transcripts");
    std::fs::create_dir_all(&transcript_dir).unwrap();
    let older = transcript_dir.join("zzzzzzzz-compact-20260101-100000.md");
    let newer = transcript_dir.join("aaaaaaaa-compact-20260102-090000.md");
    std::fs::write(&older, "# older").unwrap();
    std::fs::write(&newer, "# newer").unwrap();

    let latest = latest_transcript_artifact_path(dir.path()).unwrap();
    assert_eq!(latest, newer.display().to_string());
}

#[test]
fn runtime_task_retention_env_parser_defaults_safely() {
    assert_eq!(retention_from_env(None), 20);
    assert_eq!(retention_from_env(Some("0")), 20);
    assert_eq!(retention_from_env(Some("invalid")), 20);
    assert_eq!(retention_from_env(Some("7")), 7);
}
