use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolErrorType, ToolResult};

pub struct TaskOutputTool;

#[async_trait]
impl Tool for TaskOutputTool {
    fn name(&self) -> &str {
        "task_output"
    }

    fn user_facing_name(&self) -> &str {
        "Task Output"
    }

    fn activity_description(&self, params: &Value) -> String {
        let task_id = params
            .get("task_id")
            .and_then(|value| value.as_str())
            .unwrap_or("latest");
        format!("Reading task output: {}", task_id)
    }

    fn description(&self) -> &str {
        "Read output from a background runtime task started earlier by bash or agent. Use this instead of polling with bash."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Task ID from /tasks or prior tool metadata. If omitted, uses the latest runtime task."
                },
                "offset": {
                    "type": "integer",
                    "description": "Line number to start from (1-based, inclusive). Defaults to the last 200 lines."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to return. Defaults to 200."
                },
                "follow": {
                    "type": "boolean",
                    "default": false,
                    "description": "If true and the task is still running, wait until it finishes or timeout_secs elapses before reading output."
                },
                "timeout_secs": {
                    "type": "integer",
                    "default": 60,
                    "description": "Maximum seconds to wait when follow=true."
                }
            }
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: true,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let Some(runtime_tasks) = &ctx.runtime_tasks else {
            return Ok(ToolResult::error("Runtime task store not available.".to_string()));
        };

        let mut task_snapshot = {
            let store = runtime_tasks.lock().await;
            if let Some(task_id) = params.get("task_id").and_then(|value| value.as_str()) {
                store.get(task_id)
            } else {
                store.list().into_iter().last()
            }
        };

        let Some(mut task) = task_snapshot.take() else {
            return Ok(ToolResult::error_typed(
                "No runtime task found.".to_string(),
                ToolErrorType::NotFound,
                true,
                Some("Run /tasks to inspect available task IDs first.".to_string()),
            ));
        };

        let follow = params
            .get("follow")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let timeout_secs = params
            .get("timeout_secs")
            .and_then(|value| value.as_u64())
            .unwrap_or(60)
            .min(600);
        let mut follow_timed_out = false;
        if follow && is_unfinished_task(&task.status) {
            let deadline = tokio::time::Instant::now()
                + std::time::Duration::from_secs(timeout_secs);
            while is_unfinished_task(&task.status) {
                if tokio::time::Instant::now() >= deadline {
                    follow_timed_out = true;
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let next_snapshot = runtime_tasks.lock().await.get(&task.id);
                if let Some(next_task) = next_snapshot {
                    task = next_task;
                } else {
                    break;
                }
            }
        }

        let content = match tokio::fs::read_to_string(&task.output_path).await {
            Ok(content) => content,
            Err(err) => {
                return Ok(ToolResult::error_typed(
                    format!(
                        "Failed to read output for task {} ({}): {}",
                        task.id, task.output_path, err
                    ),
                    ToolErrorType::NotFound,
                    true,
                    Some("Check /tasks to confirm the output path still exists.".to_string()),
                ));
            }
        };

        let lines = content.lines().collect::<Vec<_>>();
        let total_lines = lines.len();
        let limit = params
            .get("limit")
            .and_then(|value| value.as_u64())
            .map(|value| value as usize)
            .unwrap_or(200);
        let start = params
            .get("offset")
            .and_then(|value| value.as_u64())
            .map(|value| value.saturating_sub(1) as usize)
            .unwrap_or_else(|| total_lines.saturating_sub(limit));
        let end = (start + limit).min(total_lines);
        let selected = lines[start.min(total_lines)..end].join("\n");
        let was_truncated = start > 0 || end < total_lines;
        let mut output = String::new();
        output.push_str(&format!(
            "Task {} [{} / {}]\nDescription: {}\nOutput path: {}\n\n",
            task.id, task.kind, format!("{:?}", task.status), task.description, task.output_path
        ));
        if !task.progress_history.is_empty() {
            output.push_str("Recent progress:\n");
            for progress in &task.progress_history {
                output.push_str(&format!("  - {}\n", progress));
            }
            output.push('\n');
        }
        output.push_str(&selected);
        if was_truncated {
            output.push_str(&format!(
                "\n\n... (showing lines {}-{} of {} total; use offset/limit to inspect more)",
                start + 1,
                end,
                total_lines
            ));
        }

        Ok(ToolResult::success_with_metadata(
            output,
            json!({
                "task_id": task.id,
                "task_kind": task.kind,
                "task_status": format!("{:?}", task.status),
                "attempt": task.attempt,
                "retry_of": task.retry_of,
                "output_path": task.output_path,
                "last_progress": task.last_progress,
                "last_progress_at": task.last_progress_at,
                "progress_history": task.progress_history,
                "follow": follow,
                "follow_timed_out": follow_timed_out,
                "total_lines": total_lines,
                "start_line": start + 1,
                "end_line": end,
                "was_truncated": was_truncated,
            }),
        ))
    }
}

fn is_unfinished_task(status: &crate::runtime_tasks::RuntimeTaskStatus) -> bool {
    matches!(
        status,
        crate::runtime_tasks::RuntimeTaskStatus::Pending
            | crate::runtime_tasks::RuntimeTaskStatus::Running
    )
}

#[cfg(test)]
mod tests {
    use super::TaskOutputTool;
    use crate::runtime_tasks::RuntimeTaskStore;
    use crate::tool::{Tool, ToolContext};
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn reads_latest_task_output() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("task.log");
        tokio::fs::write(&output, "line1\nline2\nline3\n").await.unwrap();

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
}
