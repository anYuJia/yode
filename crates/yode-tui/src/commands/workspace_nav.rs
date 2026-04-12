use std::path::{Path, PathBuf};

pub(crate) fn compact_path_badge(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_string()
}

pub(crate) fn workspace_breadcrumb(label: &str, selection: Option<&str>) -> String {
    match selection {
        Some(selection) if !selection.trim().is_empty() => {
            format!("{} -> {}", label, selection)
        }
        _ => label.to_string(),
    }
}

pub(crate) fn workspace_selection_summary(selected: usize, total: usize) -> String {
    if total == 0 {
        "0/0".to_string()
    } else {
        format!("{}/{}", selected.min(total).max(1), total)
    }
}

pub(crate) fn workspace_stale_artifact_banner(path: &Path, stale_after_minutes: i64) -> Option<String> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let modified: chrono::DateTime<chrono::Local> = modified.into();
    let age = chrono::Local::now() - modified;
    (age > chrono::Duration::minutes(stale_after_minutes)).then(|| {
        format!(
            "stale artifact: {} modified {} minutes ago",
            path.display(),
            age.num_minutes()
        )
    })
}

pub(crate) fn workspace_jump_inventory(lines: impl IntoIterator<Item = String>) -> String {
    lines.into_iter().collect::<Vec<_>>().join(" | ")
}

pub(crate) fn task_jump_targets(task_id: &str, transcript_path: Option<&str>) -> Vec<String> {
    let mut targets = vec![
        format!("/tasks {}", task_id),
        format!("/tasks read {}", task_id),
        format!("/tasks follow {}", task_id),
    ];
    if let Some(path) = transcript_path {
        targets.push(format!("/memory {}", compact_path_badge(path)));
    }
    targets
}

pub(crate) fn review_jump_targets(path: &Path) -> Vec<String> {
    vec![
        "/reviews latest".to_string(),
        format!("/reviews {}", compact_path_badge(&path.display().to_string())),
        "/memory latest".to_string(),
    ]
}

pub(crate) fn transcript_jump_targets(path: &Path) -> Vec<String> {
    vec![
        "/memory latest".to_string(),
        format!("/memory {}", compact_path_badge(&path.display().to_string())),
        "/memory compare latest latest-1".to_string(),
    ]
}

pub(crate) fn runtime_artifact_jump_targets(path: Option<&str>) -> Vec<String> {
    let mut targets = vec!["/brief".to_string(), "/status".to_string(), "/diagnostics".to_string()];
    if let Some(path) = path {
        targets.push(format!("/memory {}", compact_path_badge(path)));
    }
    targets
}

pub(crate) fn transcript_completion_targets(working_dir: &str) -> Vec<String> {
    latest_markdown_targets(Path::new(working_dir).join(".yode").join("transcripts"))
}

pub(crate) fn review_completion_targets(working_dir: &str) -> Vec<String> {
    let mut values = vec!["latest".to_string(), "list".to_string(), "summary".to_string()];
    values.extend(latest_markdown_targets(
        Path::new(working_dir).join(".yode").join("reviews"),
    ));
    values
}

fn latest_markdown_targets(dir: PathBuf) -> Vec<String> {
    let mut entries = std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
    entries
        .into_iter()
        .take(5)
        .filter_map(|path| path.file_name().and_then(|name| name.to_str()).map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        compact_path_badge, review_completion_targets, review_jump_targets,
        runtime_artifact_jump_targets, task_jump_targets, transcript_completion_targets,
        transcript_jump_targets, workspace_breadcrumb, workspace_jump_inventory,
        workspace_selection_summary,
    };

    #[test]
    fn path_badge_and_breadcrumb_compact_values() {
        assert_eq!(compact_path_badge("/tmp/demo.md"), "demo.md");
        assert_eq!(
            workspace_breadcrumb("Memory", Some("latest")),
            "Memory -> latest"
        );
    }

    #[test]
    fn selection_and_jump_inventory_render() {
        assert_eq!(workspace_selection_summary(2, 5), "2/5");
        assert!(workspace_jump_inventory(vec!["a".to_string(), "b".to_string()]).contains("a | b"));
    }

    #[test]
    fn task_review_and_transcript_targets_include_expected_commands() {
        assert!(task_jump_targets("task-1", Some("/tmp/t.md"))
            .iter()
            .any(|line| line.contains("/tasks read task-1")));
        assert!(review_jump_targets(Path::new("/tmp/review.md"))
            .iter()
            .any(|line| line.contains("/reviews latest")));
        assert!(transcript_jump_targets(Path::new("/tmp/transcript.md"))
            .iter()
            .any(|line| line.contains("/memory compare latest latest-1")));
        assert!(runtime_artifact_jump_targets(Some("/tmp/x.md"))
            .iter()
            .any(|line| line.contains("/memory x.md")));
    }

    #[test]
    fn completion_targets_list_recent_files() {
        let dir = std::env::temp_dir().join(format!("yode-workspace-nav-{}", uuid::Uuid::new_v4()));
        let transcripts = dir.join(".yode").join("transcripts");
        let reviews = dir.join(".yode").join("reviews");
        std::fs::create_dir_all(&transcripts).unwrap();
        std::fs::create_dir_all(&reviews).unwrap();
        std::fs::write(transcripts.join("aaa.md"), "x").unwrap();
        std::fs::write(reviews.join("bbb.md"), "x").unwrap();
        assert!(transcript_completion_targets(dir.to_str().unwrap())
            .iter()
            .any(|value| value == "aaa.md"));
        assert!(review_completion_targets(dir.to_str().unwrap())
            .iter()
            .any(|value| value == "bbb.md"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
