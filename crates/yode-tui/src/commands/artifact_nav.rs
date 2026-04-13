use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Local};

use crate::commands::inspector_bridge::document_from_command_output;
use crate::ui::inspector::InspectorDocument;

#[derive(Debug, Clone)]
struct ArtifactTimelineEntry {
    at: Option<SystemTime>,
    detail: String,
}

pub(crate) fn latest_artifact_by_suffix(dir: &Path, suffix: &str) -> Option<PathBuf> {
    recent_artifacts_by_suffix(dir, suffix, 1).into_iter().next()
}

pub(crate) fn recent_artifacts_by_suffix(
    dir: &Path,
    suffix: &str,
    limit: usize,
) -> Vec<PathBuf> {
    let mut entries = std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(suffix))
        })
        .collect::<Vec<_>>();
    entries.sort_by(compare_paths_by_modified_desc);
    entries.into_iter().take(limit).collect()
}

pub(crate) fn latest_workflow_execution_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(&project_root.join(".yode").join("status"), "workflow-execution.md")
}

pub(crate) fn latest_coordinator_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(&project_root.join(".yode").join("status"), "coordinate-summary.md")
        .or_else(|| {
            latest_artifact_by_suffix(
                &project_root.join(".yode").join("status"),
                "coordinate-dry-run.md",
            )
        })
}

pub(crate) fn latest_runtime_orchestration_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("status"),
        "runtime-orchestration-timeline.md",
    )
}

pub(crate) fn latest_bundle_workspace_index(cwd: &Path) -> Option<PathBuf> {
    let mut entries = std::fs::read_dir(cwd)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.join("workspace-index.md").exists())
        .map(|path| path.join("workspace-index.md"))
        .collect::<Vec<_>>();
    entries.sort_by(compare_paths_by_modified_desc);
    entries.into_iter().next()
}

pub(crate) fn resolve_artifact_basename(project_root: &Path, target: &str) -> Option<PathBuf> {
    if target.trim().is_empty() {
        return None;
    }
    let target = target.trim();
    for dir in [
        project_root.join(".yode").join("status"),
        project_root.join(".yode").join("remote"),
        project_root.join(".yode").join("startup"),
    ] {
        let mut entries = std::fs::read_dir(&dir)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name == target)
            })
            .collect::<Vec<_>>();
        entries.sort_by(compare_paths_by_modified_desc);
        if let Some(path) = entries.into_iter().next() {
            return Some(path);
        }
    }
    None
}

pub(crate) fn artifact_freshness_badge(path: &Path) -> &'static str {
    let Some(modified) = std::fs::metadata(path).ok().and_then(|meta| meta.modified().ok()) else {
        return "unknown";
    };
    let Ok(age) = SystemTime::now().duration_since(modified) else {
        return "unknown";
    };
    let minutes = age.as_secs() / 60;
    if minutes <= 10 {
        "fresh"
    } else if minutes <= 60 {
        "warm"
    } else {
        "stale"
    }
}

pub(crate) fn stale_artifact_actions(path: &Path, refresh_commands: &[String]) -> Option<String> {
    let freshness = artifact_freshness_badge(path);
    if matches!(freshness, "fresh" | "unknown") || refresh_commands.is_empty() {
        None
    } else {
        Some(format!(
            "Artifact freshness={} . Refresh with {}",
            freshness,
            refresh_commands.join(" | ")
        ))
    }
}

pub(crate) fn open_artifact_inspector(
    title: &str,
    path: &Path,
    footer: Option<String>,
    extra_badges: Vec<(String, String)>,
) -> Option<InspectorDocument> {
    let content = std::fs::read_to_string(path).ok()?;
    let lines = content.lines().map(str::to_string).collect::<Vec<_>>();
    let mut doc = document_from_command_output(title, lines);
    let mut badges = vec![
        (
            "artifact".to_string(),
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown")
                .to_string(),
        ),
        (
            "freshness".to_string(),
            artifact_freshness_badge(path).to_string(),
        ),
    ];
    badges.extend(extra_badges);
    for panel in &mut doc.panels {
        panel.badges.extend(badges.clone());
    }
    doc.footer = footer;
    Some(doc)
}

pub(crate) fn build_runtime_orchestration_timeline_lines(
    project_root: &Path,
    max_items: usize,
) -> Vec<String> {
    let mut entries = Vec::new();
    let status_dir = project_root.join(".yode").join("status");
    let remote_dir = project_root.join(".yode").join("remote");

    if let Some(path) = latest_workflow_execution_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "workflow execution"));
    }
    if let Some(path) = latest_coordinator_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "coordinator"));
    }
    if let Some(path) = latest_artifact_by_suffix(&status_dir, "runtime-timeline.md") {
        entries.push(artifact_timeline_entry(&path, "runtime timeline"));
    }
    if let Some(path) = latest_artifact_by_suffix(&remote_dir, "remote-workflow-capability.json") {
        entries.push(artifact_timeline_entry(&path, "remote capability"));
    }
    if let Some(path) = latest_artifact_by_suffix(&remote_dir, "remote-execution-state.json") {
        entries.push(artifact_timeline_entry(&path, "remote execution state"));
    }

    render_timeline_entries(entries, max_items)
}

pub(crate) fn write_runtime_orchestration_timeline_artifact(
    project_root: &Path,
    session_id: &str,
) -> Option<String> {
    let dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-runtime-orchestration-timeline.md", short_session));
    let body = build_runtime_orchestration_timeline_lines(project_root, 12)
        .into_iter()
        .map(|line| format!("- {}", line))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&path, format!("# Runtime Orchestration Timeline\n\n{}\n", body)).ok()?;
    Some(path.display().to_string())
}

fn artifact_timeline_entry(path: &Path, label: &str) -> ArtifactTimelineEntry {
    ArtifactTimelineEntry {
        at: std::fs::metadata(path).ok().and_then(|meta| meta.modified().ok()),
        detail: format!(
            "{}: {} / artifact={}",
            label,
            preview_artifact(path),
            path.display()
        ),
    }
}

fn preview_artifact(path: &Path) -> String {
    std::fs::read_to_string(path)
        .ok()
        .map(|content| {
            content
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with("```"))
                .take(2)
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .filter(|preview| !preview.is_empty())
        .unwrap_or_else(|| "no preview".to_string())
}

fn render_timeline_entries(
    mut entries: Vec<ArtifactTimelineEntry>,
    max_items: usize,
) -> Vec<String> {
    entries.sort_by(|left, right| match (&left.at, &right.at) {
        (Some(left_at), Some(right_at)) => right_at.cmp(left_at),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => left.detail.cmp(&right.detail),
    });

    if entries.is_empty() {
        return vec!["no workflow/coordinator artifacts yet".to_string()];
    }

    let hidden = entries.len().saturating_sub(max_items);
    let mut lines = entries
        .into_iter()
        .take(max_items)
        .map(|entry| {
            let at = entry
                .at
                .map(|stamp| {
                    let stamp: DateTime<Local> = stamp.into();
                    stamp.format("%Y-%m-%d %H:%M:%S").to_string()
                })
                .unwrap_or_else(|| "unknown".to_string());
            format!("{} | {}", at, entry.detail)
        })
        .collect::<Vec<_>>();
    if hidden > 0 {
        lines.push(format!("+{} earlier timeline events", hidden));
    }
    lines
}

fn compare_paths_by_modified_desc(left: &PathBuf, right: &PathBuf) -> Ordering {
    let left_modified = std::fs::metadata(left).ok().and_then(|meta| meta.modified().ok());
    let right_modified = std::fs::metadata(right).ok().and_then(|meta| meta.modified().ok());
    match (left_modified, right_modified) {
        (Some(left_modified), Some(right_modified)) => right_modified
            .cmp(&left_modified)
            .then_with(|| right.file_name().cmp(&left.file_name())),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => right.file_name().cmp(&left.file_name()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        artifact_freshness_badge, build_runtime_orchestration_timeline_lines,
        latest_artifact_by_suffix, latest_bundle_workspace_index, open_artifact_inspector,
        recent_artifacts_by_suffix, resolve_artifact_basename,
        write_runtime_orchestration_timeline_artifact,
    };

    #[test]
    fn latest_artifact_prefers_newest_modified_file() {
        let dir = std::env::temp_dir().join(format!("yode-artifact-nav-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let older = dir.join("a-workflow-execution.md");
        let newer = dir.join("b-workflow-execution.md");
        std::fs::write(&older, "old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&newer, "new").unwrap();
        let latest = latest_artifact_by_suffix(&dir, "workflow-execution.md").unwrap();
        assert_eq!(latest, newer);
        let recent = recent_artifacts_by_suffix(&dir, "workflow-execution.md", 2);
        assert_eq!(recent.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bundle_workspace_index_picks_latest_bundle() {
        let dir = std::env::temp_dir().join(format!("yode-bundle-index-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        let older = dir.join("diagnostics-a");
        let newer = dir.join("diagnostics-b");
        std::fs::create_dir_all(&older).unwrap();
        std::fs::create_dir_all(&newer).unwrap();
        std::fs::write(older.join("workspace-index.md"), "old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(newer.join("workspace-index.md"), "new").unwrap();
        let latest = latest_bundle_workspace_index(&dir).unwrap();
        assert!(latest.ends_with("diagnostics-b/workspace-index.md"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn orchestration_timeline_writes_markdown() {
        let dir = std::env::temp_dir().join(format!("yode-orchestration-{}", uuid::Uuid::new_v4()));
        let status = dir.join(".yode").join("status");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&status).unwrap();
        std::fs::write(
            status.join("session-workflow-execution.md"),
            "# Workflow Execution\n\n- Name: demo\n",
        )
        .unwrap();
        let lines = build_runtime_orchestration_timeline_lines(&dir, 4);
        assert!(lines[0].contains("workflow execution"));
        let path = write_runtime_orchestration_timeline_artifact(&dir, "session-1234").unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("Runtime Orchestration Timeline"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn artifact_inspector_applies_badges() {
        let dir = std::env::temp_dir().join(format!("yode-inspector-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("demo.md");
        std::fs::write(&path, "# Demo\n\nSummary:\n- value\n").unwrap();
        let doc = open_artifact_inspector("Demo", &path, None, vec![("kind".into(), "demo".into())])
            .unwrap();
        assert_eq!(artifact_freshness_badge(&path), "fresh");
        assert!(doc.panels[0]
            .badges
            .iter()
            .any(|(label, value)| label == "kind" && value == "demo"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn basename_resolution_searches_status_and_remote_dirs() {
        let dir = std::env::temp_dir().join(format!("yode-basename-{}", uuid::Uuid::new_v4()));
        let status = dir.join(".yode").join("status");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&status).unwrap();
        let file = status.join("demo-workflow-execution.md");
        std::fs::write(&file, "x").unwrap();
        let resolved = resolve_artifact_basename(&dir, "demo-workflow-execution.md").unwrap();
        assert_eq!(resolved, file);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
