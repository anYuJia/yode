pub(crate) fn coordinator_dry_run_prompt(goal: &str) -> String {
    format!(
        "Use `coordinate_agents` for goal=\"{}\" with dry_run=true first. Show workstreams, dependencies, and the suggested execution order before starting real work.",
        goal
    )
}

pub(crate) fn write_coordinator_stub_artifact(
    project_root: &std::path::Path,
    session_id: &str,
    goal: &str,
) -> Option<String> {
    let dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-coordinate-dry-run.md", short_session));
    let body = format!(
        "# Coordinator Dry Run\n\n- Goal: {}\n- Prompt: {}\n\nExecution outline:\n- dry_run=true\n- show workstreams, dependencies, and suggested order\n- do not start real work before reviewing the dry-run plan\n\nJump targets:\n{}\n",
        goal,
        coordinator_dry_run_prompt(goal),
        coordinator_jump_targets()
            .into_iter()
            .map(|target| format!("- {}", target))
            .collect::<Vec<_>>()
            .join("\n")
    );
    std::fs::write(&path, body).ok()?;
    Some(path.display().to_string())
}

pub(crate) fn write_coordinator_summary_artifact(
    project_root: &std::path::Path,
    session_id: &str,
    goal: &str,
    dry_run_artifact: Option<&str>,
    timeline_artifact: Option<&str>,
) -> Option<String> {
    let dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-coordinate-summary.md", short_session));
    let body = format!(
        "# Coordinator Summary\n\n- Goal: {}\n- Dry run artifact: {}\n- Timeline artifact: {}\n- Prompt: {}\n\nOperator notes:\n- start from the dry-run artifact before launching real workstreams\n- merge workflow/coordinator state through `/coordinate timeline`\n- keep remote review checks explicit when the workspace is remote\n\nJump targets:\n{}\n",
        goal,
        dry_run_artifact.unwrap_or("none"),
        timeline_artifact.unwrap_or("none"),
        coordinator_dry_run_prompt(goal),
        coordinator_jump_targets()
            .into_iter()
            .map(|target| format!("- {}", target))
            .collect::<Vec<_>>()
            .join("\n")
    );
    std::fs::write(&path, body).ok()?;
    Some(path.display().to_string())
}

pub(crate) fn coordinator_jump_targets() -> Vec<String> {
    vec![
        "/coordinate latest".to_string(),
        "/coordinate timeline".to_string(),
        "/inspect workflows latest".to_string(),
        "/inspect artifact latest-coordinate".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        coordinator_dry_run_prompt, coordinator_jump_targets, write_coordinator_stub_artifact,
        write_coordinator_summary_artifact,
    };

    #[test]
    fn coordinator_prompt_uses_dry_run() {
        assert!(coordinator_dry_run_prompt("demo").contains("dry_run=true"));
    }

    #[test]
    fn writes_coordinator_stub_artifact() {
        let dir = std::env::temp_dir().join(format!("yode-coordinate-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_coordinator_stub_artifact(&dir, "session-1234", "demo").unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("# Coordinator Dry Run"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_coordinator_summary_artifact() {
        let dir = std::env::temp_dir().join(format!("yode-coordinate-summary-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_coordinator_summary_artifact(
            &dir,
            "session-1234",
            "demo",
            Some("/tmp/dry-run.md"),
            Some("/tmp/timeline.md"),
        )
        .unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("# Coordinator Summary"));
        assert!(content.contains("/tmp/timeline.md"));
        assert!(coordinator_jump_targets()
            .iter()
            .any(|target| target.contains("/coordinate timeline")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
