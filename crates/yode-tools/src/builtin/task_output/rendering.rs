use crate::runtime_tasks::{RuntimeTask, RuntimeTaskStatus};

pub(super) fn render_task_output_header(
    task: &RuntimeTask,
    follow: bool,
    timeout_secs: u64,
    follow_timed_out: bool,
) -> String {
    let mut output = format!(
        "Task {} [{} / {:?}]\nDescription: {}\nOutput path: {}\n",
        task.id, task.kind, task.status, task.description, task.output_path
    );
    if let Some(line) = render_transcript_backlink(task.transcript_path.as_deref()) {
        output.push_str(&line);
    }
    if follow {
        output.push_str(&format!(
            "Follow mode: {}\n",
            render_follow_mode_summary(&task.status, timeout_secs, follow_timed_out)
        ));
    }
    output.push('\n');
    output
}

pub(super) fn render_transcript_backlink(transcript_path: Option<&str>) -> Option<String> {
    transcript_path.map(|path| format!("Transcript: {}\n", path))
}

pub(super) fn render_follow_mode_summary(
    status: &RuntimeTaskStatus,
    timeout_secs: u64,
    follow_timed_out: bool,
) -> String {
    if follow_timed_out {
        format!("timed out after {}s with status {:?}", timeout_secs, status)
    } else {
        format!("final status {:?} (timeout {}s)", status, timeout_secs)
    }
}
