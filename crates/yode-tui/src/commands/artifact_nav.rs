use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use std::time::SystemTime;

use chrono::{DateTime, Local};

use crate::commands::inspector_bridge::document_from_command_output;
use crate::ui::chat::render_markdown_ansi_white_with_options;
use crate::ui::inspector::{InspectorAction, InspectorDocument};

#[derive(Debug, Clone)]
struct ArtifactTimelineEntry {
    at: Option<SystemTime>,
    detail: String,
}

pub(crate) fn latest_artifact_by_suffix(dir: &Path, suffix: &str) -> Option<PathBuf> {
    recent_artifacts_by_suffix(dir, suffix, 1)
        .into_iter()
        .next()
}

pub(crate) fn recent_artifacts_by_suffix(dir: &Path, suffix: &str, limit: usize) -> Vec<PathBuf> {
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
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("status"),
        "workflow-execution.md",
    )
}

pub(crate) fn latest_checkpoint_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("checkpoints"),
        "checkpoint.md",
    )
}

pub(crate) fn latest_branch_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(&project_root.join(".yode").join("checkpoints"), "branch.md")
}

pub(crate) fn latest_rewind_anchor_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("checkpoints"),
        "rewind-anchor.md",
    )
}

pub(crate) fn latest_checkpoint_state_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("checkpoints"),
        "checkpoint-state.json",
    )
}

pub(crate) fn latest_branch_state_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("checkpoints"),
        "branch-state.json",
    )
}

pub(crate) fn latest_rewind_anchor_state_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("checkpoints"),
        "rewind-anchor-state.json",
    )
}

pub(crate) fn latest_branch_merge_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("checkpoints"),
        "branch-merge.md",
    )
}

pub(crate) fn latest_branch_merge_state_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("checkpoints"),
        "branch-merge-state.json",
    )
}

pub(crate) fn latest_remote_control_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-control.md",
    )
}

pub(crate) fn latest_remote_control_state_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-control-session.json",
    )
}

pub(crate) fn latest_remote_command_queue_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-command-queue.md",
    )
}

pub(crate) fn latest_remote_task_handoff_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-task-handoff.md",
    )
}

pub(crate) fn latest_remote_queue_execution_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-queue-execution.md",
    )
}

pub(crate) fn latest_remote_transport_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-transport.md",
    )
}

pub(crate) fn latest_remote_transport_state_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-transport-state.json",
    )
}

pub(crate) fn latest_remote_transport_events_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-transport-events.md",
    )
}

pub(crate) fn latest_remote_live_session_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-live-session.md",
    )
}

pub(crate) fn latest_remote_live_session_state_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-live-session-state.json",
    )
}

pub(crate) fn latest_remote_session_transcript_sync_artifact(
    project_root: &Path,
) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-session-transcript-sync.md",
    )
}

pub(crate) fn latest_hook_deferred_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("hooks"),
        "hook-deferred.md",
    )
}

pub(crate) fn latest_hook_deferred_state_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("hooks"),
        "hook-deferred-state.json",
    )
}

pub(crate) fn latest_permission_governance_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("hooks"),
        "permission-governance.json",
    )
}

pub(crate) fn latest_agent_team_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(&project_root.join(".yode").join("teams"), "agent-team.md")
}

pub(crate) fn latest_agent_team_state_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("teams"),
        "agent-team-state.json",
    )
}

pub(crate) fn latest_agent_team_messages_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("teams"),
        "agent-team-messages.md",
    )
}

pub(crate) fn latest_agent_team_monitor_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("teams"),
        "agent-team-monitor.md",
    )
}

pub(crate) fn latest_agent_team_bundle_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("teams"),
        "agent-team-bundle.md",
    )
}

pub(crate) fn latest_subagent_result_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(&project_root.join(".yode").join("agent-results"), ".md")
}

pub(crate) fn latest_action_history_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("status"),
        "inspector-action-history.md",
    )
}

pub(crate) fn latest_action_metrics_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("status"),
        "inspector-action-metrics.json",
    )
}

pub(crate) fn latest_prompt_cache_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("status"),
        "prompt-cache.md",
    )
}

pub(crate) fn latest_prompt_cache_state_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("status"),
        "prompt-cache-state.json",
    )
}

pub(crate) fn latest_prompt_cache_events_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("status"),
        "prompt-cache-events.md",
    )
}

pub(crate) fn latest_prompt_cache_break_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("status"),
        "prompt-cache-break.json",
    )
}

pub(crate) fn latest_prompt_cache_diff_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("status"),
        "prompt-cache-diff.md",
    )
}

pub(crate) fn latest_post_compact_restore_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("status"),
        "post-compact-restore.md",
    )
}

pub(crate) fn latest_post_compact_restore_state_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("status"),
        "post-compact-restore-state.json",
    )
}

pub(crate) fn latest_post_compact_restore_diff_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("status"),
        "post-compact-restore-diff.md",
    )
}

pub(crate) fn latest_workflow_state_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("status"),
        "workflow-runtime-state.json",
    )
}

pub(crate) fn latest_coordinator_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("status"),
        "coordinate-summary.md",
    )
    .or_else(|| {
        latest_artifact_by_suffix(
            &project_root.join(".yode").join("status"),
            "coordinate-dry-run.md",
        )
    })
}

pub(crate) fn latest_coordinator_state_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("status"),
        "coordinate-runtime-state.json",
    )
}

pub(crate) fn latest_runtime_orchestration_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("status"),
        "runtime-orchestration-timeline.md",
    )
}

pub(crate) fn latest_bundle_workspace_index(cwd: &Path) -> Option<PathBuf> {
    recent_bundle_workspace_indexes(cwd, 1).into_iter().next()
}

pub(crate) fn export_bundle_root(project_root: &Path) -> PathBuf {
    project_root.join(".yode").join("exports")
}

pub(crate) fn recent_bundle_workspace_indexes(cwd: &Path, limit: usize) -> Vec<PathBuf> {
    let mut entries = bundle_search_roots(cwd)
        .into_iter()
        .flat_map(|dir| bundle_workspace_indexes_in_dir(&dir))
        .collect::<Vec<_>>();
    entries.sort_by(compare_paths_by_modified_desc);
    entries.dedup();
    entries.into_iter().take(limit).collect()
}

fn bundle_search_roots(cwd: &Path) -> Vec<PathBuf> {
    let mut roots = cwd
        .ancestors()
        .flat_map(|dir| [export_bundle_root(dir), dir.to_path_buf()])
        .collect::<Vec<_>>();
    roots.sort();
    roots.dedup();
    roots
}

fn bundle_workspace_indexes_in_dir(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.join("workspace-index.md").exists())
        .map(|path| path.join("workspace-index.md"))
        .collect()
}

pub(crate) fn resolve_artifact_basename(project_root: &Path, target: &str) -> Option<PathBuf> {
    if target.trim().is_empty() {
        return None;
    }
    let target = target.trim();
    for dir in [
        project_root.join(".yode").join("status"),
        project_root.join(".yode").join("remote"),
        project_root.join(".yode").join("teams"),
        project_root.join(".yode").join("agent-results"),
        project_root.join(".yode").join("hooks"),
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
    let Some(modified) = std::fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
    else {
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

pub(crate) fn artifact_display_line(path: &Path) -> String {
    format!("[{}] {}", artifact_freshness_badge(path), path.display())
}

pub(crate) fn artifact_history_lines(paths: impl IntoIterator<Item = PathBuf>) -> Vec<String> {
    paths
        .into_iter()
        .map(|path| artifact_display_line(&path))
        .collect()
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
        panel.lines = panel
            .lines
            .iter()
            .flat_map(|line| render_markdown_ansi_white_with_options(line, Some(100), true))
            .collect();
    }
    doc.footer = footer;
    Some(doc)
}

pub(crate) fn attach_inspector_actions(
    doc: &mut InspectorDocument,
    actions: Vec<(String, String)>,
) {
    let actions = actions
        .into_iter()
        .map(|(label, command)| InspectorAction { label, command })
        .collect::<Vec<_>>();
    for panel in &mut doc.panels {
        panel.actions.extend(actions.clone());
    }
}

pub(crate) fn record_inspector_action_history(
    project_root: &Path,
    session_id: &str,
    command: &str,
) -> Option<String> {
    let dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-inspector-action-history.md", short_session));
    let line = format!(
        "- {} | {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        command
    );
    if path.exists() {
        let mut body = std::fs::read_to_string(&path).ok()?;
        body.push_str(&line);
        std::fs::write(&path, body).ok()?;
    } else {
        let body = format!("# Inspector Action History\n\n{}", line);
        std::fs::write(&path, body).ok()?;
    }
    let metrics_path = dir.join(format!("{}-inspector-action-metrics.json", short_session));
    let mut count = 1u64;
    let mut commands = std::collections::BTreeMap::<String, u64>::new();
    if let Ok(body) = std::fs::read_to_string(&metrics_path) {
        if let Ok(mut payload) = serde_json::from_str::<serde_json::Value>(&body) {
            count = payload
                .get("count")
                .and_then(|value| value.as_u64())
                .unwrap_or(0)
                .saturating_add(1);
            commands = payload
                .get_mut("commands")
                .and_then(|value| value.as_object_mut())
                .map(|map| {
                    map.iter()
                        .map(|(key, value)| (key.clone(), value.as_u64().unwrap_or(0)))
                        .collect()
                })
                .unwrap_or_default();
        }
    }
    *commands.entry(command.to_string()).or_default() += 1;
    let payload = serde_json::json!({
        "count": count,
        "last_command": command,
        "updated_at": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        "commands": commands,
    });
    let _ = std::fs::write(&metrics_path, serde_json::to_string_pretty(&payload).ok()?);
    Some(path.display().to_string())
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
    if let Some(path) = latest_checkpoint_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "checkpoint"));
    }
    if let Some(path) = latest_branch_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "branch"));
    }
    if let Some(path) = latest_rewind_anchor_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "rewind anchor"));
    }
    if let Some(path) = latest_branch_merge_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "branch merge"));
    }
    if let Some(path) = latest_remote_control_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "remote control"));
    }
    if let Some(path) = latest_remote_command_queue_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "remote command queue"));
    }
    if let Some(path) = latest_remote_task_handoff_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "remote task handoff"));
    }
    if let Some(path) = latest_action_history_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "action history"));
    }
    if let Some(path) = latest_action_metrics_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "action metrics"));
    }
    if let Some(path) = latest_prompt_cache_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "prompt cache"));
    }
    if let Some(path) = latest_prompt_cache_state_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "prompt cache state"));
    }
    if let Some(path) = latest_prompt_cache_events_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "prompt cache events"));
    }
    if let Some(path) = latest_prompt_cache_break_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "prompt cache break"));
    }
    if let Some(path) = latest_prompt_cache_diff_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "prompt cache diff"));
    }
    if let Some(path) = latest_prompt_cache_state_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "prompt cache state"));
    }
    if let Some(path) = latest_post_compact_restore_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "post-compact restore"));
    }
    if let Some(path) = latest_post_compact_restore_state_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "post-compact restore state"));
    }
    if let Some(path) = latest_post_compact_restore_diff_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "post-compact restore diff"));
    }
    if let Some(path) = latest_workflow_state_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "workflow state"));
    }
    if let Some(path) = latest_checkpoint_state_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "checkpoint state"));
    }
    if let Some(path) = latest_branch_state_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "branch state"));
    }
    if let Some(path) = latest_rewind_anchor_state_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "rewind anchor state"));
    }
    if let Some(path) = latest_branch_merge_state_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "branch merge state"));
    }
    if let Some(path) = latest_remote_control_state_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "remote control state"));
    }
    if let Some(path) = latest_remote_queue_execution_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "remote queue execution"));
    }
    if let Some(path) = latest_remote_transport_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "remote transport"));
    }
    if let Some(path) = latest_remote_transport_state_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "remote transport state"));
    }
    if let Some(path) = latest_remote_transport_events_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "remote transport events"));
    }
    if let Some(path) = latest_remote_live_session_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "remote live session"));
    }
    if let Some(path) = latest_remote_live_session_state_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "remote live session state"));
    }
    if let Some(path) = latest_remote_session_transcript_sync_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "remote transcript sync"));
    }
    if let Some(path) = latest_hook_deferred_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "hook deferred"));
    }
    if let Some(path) = latest_hook_deferred_state_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "hook deferred state"));
    }
    if let Some(path) = latest_agent_team_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "agent team"));
    }
    if let Some(path) = latest_agent_team_state_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "agent team state"));
    }
    if let Some(path) = latest_agent_team_messages_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "agent team messages"));
    }
    if let Some(path) = latest_agent_team_monitor_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "agent team monitor"));
    }
    if let Some(path) = latest_agent_team_bundle_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "agent team bundle"));
    }
    if let Some(path) = latest_subagent_result_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "subagent result"));
    }
    if let Some(path) = latest_coordinator_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "coordinator"));
    }
    if let Some(path) = latest_coordinator_state_artifact(project_root) {
        entries.push(artifact_timeline_entry(&path, "coordinator state"));
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
    let path = dir.join(format!(
        "{}-runtime-orchestration-timeline.md",
        short_session
    ));
    let body = build_runtime_orchestration_timeline_lines(project_root, 12)
        .into_iter()
        .map(|line| format!("- {}", line))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(
        &path,
        format!("# Runtime Orchestration Timeline\n\n{}\n", body),
    )
    .ok()?;
    Some(path.display().to_string())
}

fn artifact_timeline_entry(path: &Path, label: &str) -> ArtifactTimelineEntry {
    ArtifactTimelineEntry {
        at: std::fs::metadata(path)
            .ok()
            .and_then(|meta| meta.modified().ok()),
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
            let preview_lines = content
                .lines()
                .map(str::trim)
                .filter(|line| {
                    !line.is_empty() && !line.starts_with('#') && !line.starts_with("```")
                })
                .take(2)
                .map(|line| truncate_artifact_preview_line(line, 72))
                .collect::<Vec<_>>();
            let hidden_lines = content
                .lines()
                .map(str::trim)
                .filter(|line| {
                    !line.is_empty() && !line.starts_with('#') && !line.starts_with("```")
                })
                .count()
                .saturating_sub(preview_lines.len());
            let mut preview = preview_lines.join(" | ");
            if hidden_lines > 0 {
                preview.push_str(&format!(" | +{} more lines", hidden_lines));
            }
            preview
        })
        .filter(|preview| !preview.is_empty())
        .unwrap_or_else(|| "no preview".to_string())
}

fn truncate_artifact_preview_line(text: &str, max_chars: usize) -> String {
    let squashed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if squashed.chars().count() <= max_chars {
        return squashed;
    }
    format!(
        "{}...",
        squashed.chars().take(max_chars).collect::<String>()
    )
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
    let left_modified = std::fs::metadata(left)
        .ok()
        .and_then(|meta| meta.modified().ok());
    let right_modified = std::fs::metadata(right)
        .ok()
        .and_then(|meta| meta.modified().ok());
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
        artifact_display_line, artifact_freshness_badge,
        build_runtime_orchestration_timeline_lines, export_bundle_root, latest_artifact_by_suffix,
        latest_bundle_workspace_index, open_artifact_inspector, preview_artifact,
        recent_artifacts_by_suffix, recent_bundle_workspace_indexes, resolve_artifact_basename,
        render_timeline_entries, write_runtime_orchestration_timeline_artifact, ArtifactTimelineEntry,
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
        let newer = export_bundle_root(&dir).join("diagnostics-b");
        std::fs::create_dir_all(&older).unwrap();
        std::fs::create_dir_all(&newer).unwrap();
        std::fs::write(older.join("workspace-index.md"), "old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(newer.join("workspace-index.md"), "new").unwrap();
        let latest = latest_bundle_workspace_index(&dir).unwrap();
        assert!(latest.ends_with(".yode/exports/diagnostics-b/workspace-index.md"));
        assert_eq!(recent_bundle_workspace_indexes(&dir, 2).len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bundle_workspace_index_searches_export_root_from_nested_cwd() {
        let dir = std::env::temp_dir().join(format!("yode-bundle-nested-{}", uuid::Uuid::new_v4()));
        let nested = dir.join("crates").join("demo");
        let bundle = export_bundle_root(&dir).join("diagnostics-nested");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("workspace-index.md"), "nested").unwrap();

        let latest = latest_bundle_workspace_index(&nested).unwrap();
        assert!(latest.ends_with(".yode/exports/diagnostics-nested/workspace-index.md"));
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
        std::fs::write(&path, "# Demo\n\nSummary:\n- value\n- https://example.com\n").unwrap();
        let doc =
            open_artifact_inspector("Demo", &path, None, vec![("kind".into(), "demo".into())])
                .unwrap();
        assert_eq!(artifact_freshness_badge(&path), "fresh");
        assert!(artifact_display_line(&path).contains("[fresh]"));
        assert!(doc.panels[0]
            .badges
            .iter()
            .any(|(label, value)| label == "kind" && value == "demo"));
        assert!(doc
            .panels
            .iter()
            .flat_map(|panel| panel.lines.iter())
            .any(|line| line.contains("\u{1b}]8;;https://example.com")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn artifact_preview_truncates_and_reports_hidden_lines() {
        let dir = std::env::temp_dir().join(format!("yode-artifact-preview-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("preview.md");
        std::fs::write(
            &path,
            "# Demo\n\nThis is a very long first line that should be truncated because it keeps going past the preview budget for artifact timeline summaries.\nSecond line stays visible.\nThird line should be counted as hidden.\n",
        )
        .unwrap();
        let preview = preview_artifact(&path);
        assert!(preview.contains("This is a very long first line"));
        assert!(preview.contains("Second line stays visible."));
        assert!(preview.contains("+1 more lines"));
        assert!(preview.contains("..."));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn artifact_timeline_entries_sort_newest_first_and_fold_hidden_items() {
        let now = std::time::SystemTime::now();
        let lines = render_timeline_entries(
            vec![
                ArtifactTimelineEntry {
                    at: Some(now - std::time::Duration::from_secs(120)),
                    detail: "older".to_string(),
                },
                ArtifactTimelineEntry {
                    at: Some(now),
                    detail: "newest".to_string(),
                },
                ArtifactTimelineEntry {
                    at: Some(now - std::time::Duration::from_secs(60)),
                    detail: "middle".to_string(),
                },
            ],
            2,
        );
        assert!(lines[0].contains("newest"));
        assert!(lines[1].contains("middle"));
        assert_eq!(lines[2], "+1 earlier timeline events");
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
