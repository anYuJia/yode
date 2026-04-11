use anyhow::Result;
use serde_json::{json, Value};

use crate::tool::{ToolContext, ToolErrorType, ToolResult};

pub(super) async fn execute_task_output(params: Value, ctx: &ToolContext) -> Result<ToolResult> {
    let Some(runtime_tasks) = &ctx.runtime_tasks else {
        return Ok(ToolResult::error(
            "Runtime task store not available.".to_string(),
        ));
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
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
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
        Err(error) => {
            return Ok(ToolResult::error_typed(
                format!(
                    "Failed to read output for task {} ({}): {}",
                    task.id, task.output_path, error
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
    let explicit_offset = params.get("offset").is_some();
    let (selected, start_line, end_line, was_truncated, folded_agent_output) =
        select_task_output_lines(&task.kind, &lines, start, limit, explicit_offset);
    let mut output = String::new();
    output.push_str(&format!(
        "Task {} [{} / {}]\nDescription: {}\nOutput path: {}\n",
        task.id,
        task.kind,
        format!("{:?}", task.status),
        task.description,
        task.output_path
    ));
    if let Some(transcript_path) = &task.transcript_path {
        output.push_str(&format!("Transcript: {}\n", transcript_path));
    }
    output.push('\n');
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
            start_line, end_line, total_lines
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
            "transcript_path": task.transcript_path,
            "last_progress": task.last_progress,
            "last_progress_at": task.last_progress_at,
            "progress_history": task.progress_history,
            "follow": follow,
            "follow_timed_out": follow_timed_out,
            "total_lines": total_lines,
            "start_line": start_line,
            "end_line": end_line,
            "was_truncated": was_truncated,
            "folded_agent_output": folded_agent_output,
        }),
    ))
}

fn is_unfinished_task(status: &crate::runtime_tasks::RuntimeTaskStatus) -> bool {
    matches!(
        status,
        crate::runtime_tasks::RuntimeTaskStatus::Pending
            | crate::runtime_tasks::RuntimeTaskStatus::Running
    )
}

pub(super) fn select_task_output_lines(
    task_kind: &str,
    lines: &[&str],
    start: usize,
    limit: usize,
    explicit_offset: bool,
) -> (String, usize, usize, bool, bool) {
    let total_lines = lines.len();
    if task_kind == "agent" && !explicit_offset && total_lines > limit && limit >= 40 {
        let head_count = 12usize.min(limit / 3);
        let tail_count = limit.saturating_sub(head_count);
        let mut selected = lines[..head_count].join("\n");
        selected.push_str(&format!(
            "\n\n... [agent output folded: {} middle lines omitted] ...\n\n",
            total_lines.saturating_sub(head_count + tail_count)
        ));
        selected.push_str(&lines[total_lines - tail_count..].join("\n"));
        return (selected, 1, total_lines, true, true);
    }

    let bounded_start = start.min(total_lines);
    let end = (bounded_start + limit).min(total_lines);
    (
        lines[bounded_start..end].join("\n"),
        bounded_start + 1,
        end,
        bounded_start > 0 || end < total_lines,
        false,
    )
}
