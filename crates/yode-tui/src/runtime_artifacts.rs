pub(crate) fn write_runtime_task_inventory_artifact(
    project_root: &std::path::Path,
    session_id: &str,
    tasks: Vec<yode_tools::RuntimeTask>,
) -> Option<String> {
    if tasks.is_empty() {
        return None;
    }
    let dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-runtime-tasks.md", short_session));
    let mut body = format!("# Runtime Task Inventory\n\n- Total tasks: {}\n\n", tasks.len());
    for task in tasks {
        body.push_str(&format!(
            "## {}\n\n- Kind: {}\n- Status: {:?}\n- Description: {}\n- Output: {}\n- Transcript: {}\n\n",
            task.id,
            task.kind,
            task.status,
            task.description,
            task.output_path,
            task.transcript_path.as_deref().unwrap_or("none"),
        ));
    }
    std::fs::write(&path, body).ok()?;
    Some(path.display().to_string())
}

#[cfg(test)]
mod tests {
    use yode_tools::{RuntimeTask, RuntimeTaskStatus};

    use super::write_runtime_task_inventory_artifact;

    #[test]
    fn writes_runtime_task_inventory_markdown() {
        let dir = std::env::temp_dir().join(format!(
            "yode-runtime-artifacts-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let path = write_runtime_task_inventory_artifact(
            &dir,
            "session-1234",
            vec![RuntimeTask {
                id: "task-1".to_string(),
                kind: "bash".to_string(),
                source_tool: "bash".to_string(),
                description: "run tests".to_string(),
                status: RuntimeTaskStatus::Completed,
                attempt: 1,
                retry_of: None,
                output_path: "/tmp/task.log".to_string(),
                transcript_path: Some("/tmp/task.md".to_string()),
                created_at: "2026-01-01 00:00:00".to_string(),
                started_at: None,
                completed_at: None,
                last_progress: None,
                last_progress_at: None,
                progress_history: Vec::new(),
                error: None,
            }],
        )
        .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Runtime Task Inventory"));
        assert!(content.contains("task-1"));
        assert!(content.contains("/tmp/task.md"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
