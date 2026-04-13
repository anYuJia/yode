use std::collections::BTreeMap;

use yode_core::engine::EngineRuntimeState;
use yode_tools::{RuntimeTask, RuntimeTaskStatus};

use crate::commands::info::shared::parse_runtime_timestamp;

pub(super) fn grouped_task_runtime_summary(tasks: &[RuntimeTask]) -> Vec<String> {
    let mut by_kind = BTreeMap::<String, (usize, usize, usize, usize, usize)>::new();
    for task in tasks {
        let entry = by_kind
            .entry(task.kind.clone())
            .or_insert((0, 0, 0, 0, 0));
        entry.0 += 1;
        match task.status {
            RuntimeTaskStatus::Pending => entry.1 += 1,
            RuntimeTaskStatus::Running => entry.2 += 1,
            RuntimeTaskStatus::Completed => entry.3 += 1,
            RuntimeTaskStatus::Failed | RuntimeTaskStatus::Cancelled => entry.4 += 1,
        }
    }

    if by_kind.is_empty() {
        return vec!["none".to_string()];
    }

    by_kind
        .into_iter()
        .map(|(kind, (total, pending, running, completed, issue))| {
            format!(
                "{}: total={} pending={} running={} completed={} issue={}",
                kind, total, pending, running, completed, issue
            )
        })
        .collect()
}

pub(super) fn task_notification_summary(tasks: &[RuntimeTask]) -> Vec<String> {
    let mut tasks = tasks.to_vec();
    tasks.sort_by(|left, right| latest_activity(right).cmp(&latest_activity(left)));
    let lines = tasks
        .into_iter()
        .filter(|task| {
            matches!(
                task.status,
                RuntimeTaskStatus::Completed
                    | RuntimeTaskStatus::Failed
                    | RuntimeTaskStatus::Cancelled
            )
        })
        .take(5)
        .map(|task| {
            let badge = match task.status {
                RuntimeTaskStatus::Completed => "success",
                RuntimeTaskStatus::Failed => "error",
                RuntimeTaskStatus::Cancelled => "warning",
                RuntimeTaskStatus::Pending => "pending",
                RuntimeTaskStatus::Running => "running",
            };
            format!(
                "[{}] {} {}",
                badge,
                task.id,
                task.error
                    .as_deref()
                    .unwrap_or(task.description.as_str())
            )
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        vec!["none".to_string()]
    } else {
        lines
    }
}

pub(super) fn runtime_freshness_banner(
    tasks: &[RuntimeTask],
    runtime: Option<&EngineRuntimeState>,
) -> String {
    let task_latest = tasks
        .iter()
        .filter_map(|task| parse_runtime_timestamp(Some(latest_activity(task))))
        .max();
    let runtime_latest = runtime
        .and_then(|state| parse_runtime_timestamp(state.last_tool_turn_completed_at.as_deref()));
    let latest = task_latest.into_iter().chain(runtime_latest).max();

    let Some(latest) = latest else {
        return "runtime freshness: unknown".to_string();
    };
    let age = chrono::Local::now().naive_local() - latest;
    if age.num_minutes() <= 10 {
        format!("runtime freshness: fresh ({}m)", age.num_minutes())
    } else if age.num_minutes() <= 60 {
        format!("runtime freshness: warm ({}m)", age.num_minutes())
    } else {
        format!("runtime freshness: stale ({}m)", age.num_minutes())
    }
}

pub(super) fn task_follow_prompt(task_id: &str) -> String {
    format!(
        "Use `task_output` with task_id=\"{}\", follow=true, and timeout_secs=120. Summarize final status, retries, artifact paths, and the most important output.",
        task_id
    )
}

pub(super) fn task_issue_template(task: &RuntimeTask) -> String {
    format!(
        "# Task Runtime Issue\n\n- Task: {}\n- Kind: {}\n- Source tool: {}\n- Status: {:?}\n- Retry chain: attempt {}{}\n- Output: {}\n- Transcript: {}\n- Last progress: {}\n- Error: {}\n\n## Reproduction / Context\n\n- Describe what triggered this task.\n- Include relevant runtime timeline or diagnostics snippets.\n\n## Expected\n\n- Describe the expected task outcome.\n\n## Actual\n\n- Summarize the observed outcome and attach the output/transcript artifacts above.\n",
        task.id,
        task.kind,
        task.source_tool,
        task.status,
        task.attempt,
        task.retry_of
            .as_ref()
            .map(|retry| format!(" (retry of {})", retry))
            .unwrap_or_default(),
        task.output_path,
        task.transcript_path.as_deref().unwrap_or("none"),
        task.last_progress.as_deref().unwrap_or("none"),
        task.error.as_deref().unwrap_or("none"),
    )
}

fn latest_activity(task: &RuntimeTask) -> &str {
    task.completed_at
        .as_deref()
        .or(task.last_progress_at.as_deref())
        .or(task.started_at.as_deref())
        .unwrap_or(task.created_at.as_str())
}

#[cfg(test)]
mod tests {
    use yode_tools::{RuntimeTask, RuntimeTaskStatus};

    use super::{
        grouped_task_runtime_summary, runtime_freshness_banner, task_follow_prompt,
        task_issue_template, task_notification_summary,
    };

    fn make_task(id: &str, kind: &str, status: RuntimeTaskStatus, at: &str) -> RuntimeTask {
        RuntimeTask {
            id: id.to_string(),
            kind: kind.to_string(),
            source_tool: kind.to_string(),
            description: format!("desc {}", id),
            status,
            attempt: 1,
            retry_of: None,
            output_path: format!("/tmp/{}.log", id),
            transcript_path: Some(format!("/tmp/{}.md", id)),
            created_at: at.to_string(),
            started_at: Some(at.to_string()),
            completed_at: Some(at.to_string()),
            last_progress: Some("done".to_string()),
            last_progress_at: Some(at.to_string()),
            progress_history: vec!["done".to_string()],
            error: None,
        }
    }

    #[test]
    fn grouped_summary_counts_by_kind() {
        let lines = grouped_task_runtime_summary(&[
            make_task("1", "bash", RuntimeTaskStatus::Completed, "2026-01-01 00:00:00"),
            make_task("2", "bash", RuntimeTaskStatus::Failed, "2026-01-01 00:00:01"),
        ]);
        assert!(lines[0].contains("bash: total=2"));
    }

    #[test]
    fn notification_summary_prefers_recent_finished_tasks() {
        let lines = task_notification_summary(&[
            make_task("1", "bash", RuntimeTaskStatus::Completed, "2026-01-01 00:00:00"),
            make_task("2", "bash", RuntimeTaskStatus::Failed, "2026-01-01 00:00:01"),
        ]);
        assert!(lines[0].contains("[error]"));
    }

    #[test]
    fn follow_prompt_and_issue_template_include_task_context() {
        let task = make_task("1", "bash", RuntimeTaskStatus::Failed, "2026-01-01 00:00:00");
        assert!(task_follow_prompt("task-1").contains("task_output"));
        assert!(task_issue_template(&task).contains("# Task Runtime Issue"));
    }

    #[test]
    fn freshness_banner_formats_age() {
        let line = runtime_freshness_banner(
            &[make_task(
                "1",
                "bash",
                RuntimeTaskStatus::Completed,
                &chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            )],
            None,
        );
        assert!(line.contains("runtime freshness:"));
    }
}
