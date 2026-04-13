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
        "# Coordinator Dry Run\n\n- Goal: {}\n- Prompt: {}\n",
        goal,
        coordinator_dry_run_prompt(goal)
    );
    std::fs::write(&path, body).ok()?;
    Some(path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::{coordinator_dry_run_prompt, write_coordinator_stub_artifact};

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
}
