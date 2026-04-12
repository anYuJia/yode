use std::sync::Arc;

use serde_json::json;
use tokio::sync::Mutex;

use super::execution::select_task_output_lines;
use super::TaskOutputTool;
use crate::runtime_tasks::RuntimeTaskStore;
use crate::tool::{Tool, ToolContext};

#[tokio::test]
async fn reads_latest_task_output() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("task.log");
    tokio::fs::write(&output, "line1\nline2\nline3\n")
        .await
        .unwrap();

    let store = Arc::new(Mutex::new(RuntimeTaskStore::new()));
    let task_id = {
        let mut guard = store.lock().await;
        let (task, _cancel_rx) = guard.create(
            "bash".to_string(),
            "bash".to_string(),
            "demo task".to_string(),
            output.display().to_string(),
        );
        guard.mark_completed(&task.id);
        task.id
    };

    let mut ctx = ToolContext::empty();
    ctx.runtime_tasks = Some(store);

    let tool = TaskOutputTool;
    let result = tool
        .execute(json!({ "task_id": task_id, "limit": 2 }), &ctx)
        .await
        .unwrap();
    assert!(!result.is_error);
    assert!(result.content.contains("line2"));
    assert!(result.content.contains("line3"));
}

#[tokio::test]
async fn follows_running_task_until_completion() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("task.log");
    tokio::fs::write(&output, "line1\n").await.unwrap();

    let store = Arc::new(Mutex::new(RuntimeTaskStore::new()));
    let task_id = {
        let mut guard = store.lock().await;
        let (task, _cancel_rx) = guard.create(
            "bash".to_string(),
            "bash".to_string(),
            "demo task".to_string(),
            output.display().to_string(),
        );
        guard.mark_running(&task.id);
        task.id
    };

    let store_for_task = Arc::clone(&store);
    let task_id_for_task = task_id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        tokio::fs::write(&output, "line1\nline2\n").await.unwrap();
        store_for_task
            .lock()
            .await
            .mark_completed(&task_id_for_task);
    });

    let mut ctx = ToolContext::empty();
    ctx.runtime_tasks = Some(store);

    let tool = TaskOutputTool;
    let result = tool
        .execute(
            json!({ "task_id": task_id, "follow": true, "timeout_secs": 2 }),
            &ctx,
        )
        .await
        .unwrap();
    assert!(!result.is_error);
    assert!(result.content.contains("line2"));
    assert_eq!(result.metadata.unwrap()["follow_timed_out"], false);
}

#[tokio::test]
async fn includes_transcript_backlink_in_output_and_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("task.log");
    tokio::fs::write(&output, "line1\n").await.unwrap();

    let store = Arc::new(Mutex::new(RuntimeTaskStore::new()));
    let task_id = {
        let mut guard = store.lock().await;
        let (task, _cancel_rx) = guard.create_with_transcript(
            "agent".to_string(),
            "agent".to_string(),
            "demo task".to_string(),
            output.display().to_string(),
            Some("/tmp/transcript.md".to_string()),
        );
        guard.mark_completed(&task.id);
        task.id
    };

    let mut ctx = ToolContext::empty();
    ctx.runtime_tasks = Some(store);

    let tool = TaskOutputTool;
    let result = tool
        .execute(json!({ "task_id": task_id }), &ctx)
        .await
        .unwrap();
    assert!(!result.is_error);
    assert!(result.content.contains("Transcript: /tmp/transcript.md"));
    assert_eq!(
        result.metadata.as_ref().unwrap()["transcript_path"],
        "/tmp/transcript.md"
    );
}

#[test]
fn folds_long_agent_output_by_default() {
    let lines = (0..120).map(|i| format!("line {}", i)).collect::<Vec<_>>();
    let refs = lines.iter().map(String::as_str).collect::<Vec<_>>();
    let (selected, start, end, truncated, folded) =
        select_task_output_lines("agent", &refs, 0, 60, false);
    assert_eq!(start, 1);
    assert_eq!(end, 120);
    assert!(truncated);
    assert!(folded);
    assert!(selected.contains("line 0"));
    assert!(selected.contains("line 119"));
    assert!(selected.contains("agent output folded"));
}
