use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use yode_llm::types::{Message, Role};

use crate::app::{ChatEntry, ChatRole};
use crate::commands::artifact_nav::{
    artifact_freshness_badge, latest_artifact_by_suffix, latest_coordinator_artifact,
    latest_runtime_orchestration_artifact, latest_workflow_execution_artifact,
};
use crate::commands::workspace_text::{workspace_bullets, WorkspaceText};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CheckpointMessage {
    pub index: usize,
    pub role: String,
    pub content: String,
    pub reasoning: Option<String>,
    pub tool_metadata: Option<serde_json::Value>,
    pub tool_error_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct CheckpointArtifacts {
    pub latest_review: Option<String>,
    pub latest_transcript: Option<String>,
    pub latest_workflow: Option<String>,
    pub latest_coordinate: Option<String>,
    pub latest_orchestration: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct CheckpointLineage {
    pub branch_name: Option<String>,
    pub source_kind: Option<String>,
    pub source_label: Option<String>,
    pub source_summary_artifact: Option<String>,
    pub rewind_target_label: Option<String>,
    pub transcript_anchor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SessionCheckpointPayload {
    pub schema_version: u32,
    pub kind: String,
    pub label: String,
    pub session_id: String,
    pub provider: String,
    pub model: String,
    pub working_dir: String,
    pub created_at: String,
    pub message_count: usize,
    pub role_counts: BTreeMap<String, usize>,
    pub artifacts: CheckpointArtifacts,
    #[serde(default)]
    pub lineage: CheckpointLineage,
    #[serde(default)]
    pub engine_messages: Vec<Message>,
    pub messages: Vec<CheckpointMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BranchMergePreview {
    pub kind: String,
    pub branch_label: String,
    pub current_label: String,
    pub common_prefix_messages: usize,
    pub branch_only_messages: usize,
    pub current_only_messages: usize,
    pub merged_message_count: usize,
    pub conflicts: Vec<String>,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BranchMergeExecutionPayload {
    pub kind: String,
    pub branch_label: String,
    pub current_label: String,
    pub merged_message_count: usize,
    pub current_tail_count: usize,
    pub branch_tail_count: usize,
    pub merged_at: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionCheckpointArtifactSet {
    pub summary_path: PathBuf,
    pub state_path: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct CheckpointInventoryEntry {
    pub summary_path: PathBuf,
    pub state_path: PathBuf,
    pub payload: SessionCheckpointPayload,
}

pub(crate) fn write_session_checkpoint(
    project_root: &Path,
    session_id: &str,
    provider: &str,
    model: &str,
    label: &str,
    chat_entries: &[ChatEntry],
    engine_messages: &[Message],
) -> anyhow::Result<SessionCheckpointArtifactSet> {
    let dir = checkpoint_dir(project_root);
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let short_session = session_id.chars().take(8).collect::<String>();
    let slug = checkpoint_slug(label);
    let base = format!("{}-{}-{}", stamp, short_session, slug);
    let state_path = dir.join(format!("{}-checkpoint-state.json", base));
    let summary_path = dir.join(format!("{}-checkpoint.md", base));

    let payload = build_checkpoint_payload(
        project_root,
        session_id,
        provider,
        model,
        label,
        chat_entries,
        engine_messages,
    );
    std::fs::write(&state_path, serde_json::to_string_pretty(&payload)?)?;
    std::fs::write(&summary_path, render_checkpoint_summary(&payload, &state_path, None))?;

    Ok(SessionCheckpointArtifactSet {
        summary_path,
        state_path,
    })
}

pub(crate) fn checkpoint_inventory(project_root: &Path, limit: usize) -> Vec<CheckpointInventoryEntry> {
    recent_snapshot_summary_paths(project_root, &["checkpoint.md"], limit)
        .into_iter()
        .filter_map(|summary_path| {
            let state_path = summary_path_to_state_path(&summary_path)?;
            let payload = load_checkpoint_payload(&state_path).ok()?;
            Some(CheckpointInventoryEntry {
                summary_path,
                state_path,
                payload,
            })
        })
        .collect()
}

pub(crate) fn branch_inventory(project_root: &Path, limit: usize) -> Vec<CheckpointInventoryEntry> {
    recent_snapshot_summary_paths(project_root, &["branch.md"], limit)
        .into_iter()
        .filter_map(|summary_path| {
            let state_path = summary_path_to_state_path(&summary_path)?;
            let payload = load_checkpoint_payload(&state_path).ok()?;
            Some(CheckpointInventoryEntry {
                summary_path,
                state_path,
                payload,
            })
        })
        .collect()
}

pub(crate) fn rewind_anchor_inventory(
    project_root: &Path,
    limit: usize,
) -> Vec<CheckpointInventoryEntry> {
    recent_snapshot_summary_paths(project_root, &["rewind-anchor.md"], limit)
        .into_iter()
        .filter_map(|summary_path| {
            let state_path = summary_path_to_state_path(&summary_path)?;
            let payload = load_checkpoint_payload(&state_path).ok()?;
            Some(CheckpointInventoryEntry {
                summary_path,
                state_path,
                payload,
            })
        })
        .collect()
}

pub(crate) fn rollback_anchor_inventory(
    project_root: &Path,
    limit: usize,
) -> Vec<CheckpointInventoryEntry> {
    recent_snapshot_summary_paths(project_root, &["restore-rollback.md", "merge-rollback.md"], limit)
        .into_iter()
        .filter_map(|summary_path| {
            let state_path = summary_path_to_state_path(&summary_path)?;
            let payload = load_checkpoint_payload(&state_path).ok()?;
            Some(CheckpointInventoryEntry {
                summary_path,
                state_path,
                payload,
            })
        })
        .collect()
}

pub(crate) fn resolve_checkpoint_target(
    project_root: &Path,
    target: &str,
) -> Option<CheckpointInventoryEntry> {
    let trimmed = target.trim();
    let entries = checkpoint_inventory(project_root, 32);
    if entries.is_empty() {
        return None;
    }

    if trimmed.is_empty() || trimmed == "latest" {
        return entries.into_iter().next();
    }
    if let Some(offset) = trimmed
        .strip_prefix("latest-")
        .and_then(|value| value.parse::<usize>().ok())
    {
        return entries.into_iter().nth(offset);
    }
    if let Ok(index) = trimmed.parse::<usize>() {
        return index.checked_sub(1).and_then(|idx| entries.into_iter().nth(idx));
    }

    entries.into_iter().find(|entry| {
        entry
            .summary_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == trimmed)
            || entry
                .state_path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == trimmed)
    })
}

pub(crate) fn resolve_branch_target(
    project_root: &Path,
    target: &str,
) -> Option<CheckpointInventoryEntry> {
    let trimmed = target.trim();
    let entries = branch_inventory(project_root, 32);
    resolve_snapshot_target(entries, trimmed)
}

pub(crate) fn resolve_rewind_anchor_target(
    project_root: &Path,
    target: &str,
) -> Option<CheckpointInventoryEntry> {
    let trimmed = target.trim();
    let entries = rewind_anchor_inventory(project_root, 32);
    resolve_snapshot_target(entries, trimmed)
}

pub(crate) fn resolve_rollback_anchor_target(
    project_root: &Path,
    target: &str,
) -> Option<CheckpointInventoryEntry> {
    let trimmed = target.trim();
    let entries = rollback_anchor_inventory(project_root, 32);
    resolve_snapshot_target(entries, trimmed)
}

fn resolve_snapshot_target(
    entries: Vec<CheckpointInventoryEntry>,
    target: &str,
) -> Option<CheckpointInventoryEntry> {
    if entries.is_empty() {
        return None;
    }
    let trimmed = target.trim();
    if trimmed.is_empty() || trimmed == "latest" {
        return entries.into_iter().next();
    }
    if let Some(offset) = trimmed
        .strip_prefix("latest-")
        .and_then(|value| value.parse::<usize>().ok())
    {
        return entries.into_iter().nth(offset);
    }
    if let Ok(index) = trimmed.parse::<usize>() {
        return index.checked_sub(1).and_then(|idx| entries.into_iter().nth(idx));
    }

    entries.into_iter().find(|entry| {
        entry
            .summary_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == trimmed)
            || entry
                .state_path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == trimmed)
            || entry
                .payload
                .lineage
                .branch_name
                .as_deref()
                .is_some_and(|name| name == trimmed)
    })
}

pub(crate) fn checkpoint_completion_targets(working_dir: &str) -> Vec<String> {
    let root = PathBuf::from(working_dir);
    let mut values = vec![
        "save".to_string(),
        "list".to_string(),
        "latest".to_string(),
        "branch list".to_string(),
        "branch latest".to_string(),
        "branch save workstream-a".to_string(),
        "branch diff latest latest-1".to_string(),
        "branch merge-dry-run latest".to_string(),
        "rewind-anchor".to_string(),
        "rewind-anchor list".to_string(),
        "rewind-anchor latest".to_string(),
        "rewind-anchor save latest".to_string(),
        "rewind latest".to_string(),
        "rollback list".to_string(),
        "rollback latest".to_string(),
        "rollback-dry-run latest".to_string(),
        "restore latest".to_string(),
        "diff latest latest-1".to_string(),
        "restore-dry-run latest".to_string(),
    ];
    for entry in checkpoint_inventory(&root, 6) {
        if let Some(name) = entry.summary_path.file_name().and_then(|name| name.to_str()) {
            values.push(name.to_string());
        }
    }
    for entry in branch_inventory(&root, 6) {
        if let Some(name) = entry.summary_path.file_name().and_then(|name| name.to_str()) {
            values.push(format!("branch {}", name));
        }
    }
    for entry in rewind_anchor_inventory(&root, 4) {
        if let Some(name) = entry.summary_path.file_name().and_then(|name| name.to_str()) {
            values.push(format!("rewind-anchor {}", name));
        }
    }
    values
}

pub(crate) fn render_checkpoint_list(project_root: &Path) -> String {
    let entries = checkpoint_inventory(project_root, 12);
    if entries.is_empty() {
        return format!(
            "No session checkpoints found in {}.",
            checkpoint_dir(project_root).display()
        );
    }

    let mut out = format!(
        "Session checkpoints in {}:\n",
        checkpoint_dir(project_root).display()
    );
    for (index, entry) in entries.iter().enumerate() {
        out.push_str(&format!(
            "  {:>2}. [{}] {} · {} msg · {}\n",
            index + 1,
            artifact_freshness_badge(&entry.summary_path),
            entry.payload.label,
            entry.payload.message_count,
            entry.summary_path.display()
        ));
    }
    out.push_str("\nUse `/checkpoint latest`, `/checkpoint diff latest latest-1`, or `/checkpoint restore-dry-run latest`.");
    out
}

pub(crate) fn render_branch_list(project_root: &Path) -> String {
    let entries = branch_inventory(project_root, 12);
    if entries.is_empty() {
        return format!(
            "No session branches found in {}.",
            checkpoint_dir(project_root).display()
        );
    }

    let mut out = format!("Session branches in {}:\n", checkpoint_dir(project_root).display());
    for (index, entry) in entries.iter().enumerate() {
        out.push_str(&format!(
            "  {:>2}. [{}] {} · source={} · {}\n",
            index + 1,
            artifact_freshness_badge(&entry.summary_path),
            entry.payload
                .lineage
                .branch_name
                .as_deref()
                .unwrap_or(entry.payload.label.as_str()),
            entry.payload
                .lineage
                .source_label
                .as_deref()
                .unwrap_or("current session"),
            entry.summary_path.display()
        ));
    }
    out.push_str("\nUse `/checkpoint branch latest` or `/checkpoint branch diff latest latest-1`.");
    out
}

pub(crate) fn render_rewind_anchor_list(project_root: &Path) -> String {
    let entries = rewind_anchor_inventory(project_root, 12);
    if entries.is_empty() {
        return format!(
            "No rewind anchors found in {}.",
            checkpoint_dir(project_root).display()
        );
    }
    let mut out = format!("Rewind anchors in {}:\n", checkpoint_dir(project_root).display());
    for (index, entry) in entries.iter().enumerate() {
        out.push_str(&format!(
            "  {:>2}. [{}] {} · target={} · {}\n",
            index + 1,
            artifact_freshness_badge(&entry.summary_path),
            entry.payload.label,
            entry.payload
                .lineage
                .rewind_target_label
                .as_deref()
                .unwrap_or("latest"),
            entry.summary_path.display()
        ));
    }
    out.push_str("\nUse `/checkpoint rewind latest` or `/checkpoint rewind-anchor latest`.");
    out
}

pub(crate) fn render_rollback_anchor_list(project_root: &Path) -> String {
    let entries = rollback_anchor_inventory(project_root, 12);
    if entries.is_empty() {
        return format!(
            "No rollback anchors found in {}.",
            checkpoint_dir(project_root).display()
        );
    }
    let mut out = format!("Rollback anchors in {}:\n", checkpoint_dir(project_root).display());
    for (index, entry) in entries.iter().enumerate() {
        out.push_str(&format!(
            "  {:>2}. [{}] {} · source={} · {}\n",
            index + 1,
            artifact_freshness_badge(&entry.summary_path),
            entry.payload.label,
            entry
                .payload
                .lineage
                .source_label
                .as_deref()
                .unwrap_or("current session"),
            entry.summary_path.display()
        ));
    }
    out.push_str("\nUse `/checkpoint rollback-dry-run latest` or `/inspect artifact latest-rollback-anchor`.");
    out
}

pub(crate) fn render_checkpoint_diff(
    left: &SessionCheckpointPayload,
    right: &SessionCheckpointPayload,
    left_label: &str,
    right_label: &str,
) -> String {
    let mut role_lines = Vec::new();
    let keys = left
        .role_counts
        .keys()
        .chain(right.role_counts.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    for key in keys {
        let left_count = left.role_counts.get(&key).copied().unwrap_or(0);
        let right_count = right.role_counts.get(&key).copied().unwrap_or(0);
        role_lines.push(format!("{}: {} -> {}", key, left_count, right_count));
    }

    WorkspaceText::new("Session checkpoint diff")
        .subtitle(format!("{} <> {}", left_label, right_label))
        .field(
            "Messages",
            format!("{} -> {}", left.message_count, right.message_count),
        )
        .field(
            "Provider/model",
            format!("{}:{} -> {}:{}", left.provider, left.model, right.provider, right.model),
        )
        .field(
            "Working dir",
            format!("{} -> {}", left.working_dir, right.working_dir),
        )
        .section("Role counts", workspace_bullets(role_lines))
        .section(
            "Tail preview",
            workspace_bullets([
                format!("left: {}", checkpoint_tail_preview(left)),
                format!("right: {}", checkpoint_tail_preview(right)),
            ]),
        )
        .footer(checkpoint_operator_guide())
        .render()
}

pub(crate) fn render_restore_dry_run(
    current: &SessionCheckpointPayload,
    target: &SessionCheckpointPayload,
    target_label: &str,
) -> String {
    WorkspaceText::new("Checkpoint restore dry run")
        .subtitle(target_label.to_string())
        .field("Mutation", "none (preview only)")
        .field(
            "Current messages",
            current.message_count.to_string(),
        )
        .field(
            "Checkpoint messages",
            target.message_count.to_string(),
        )
        .section(
            "What would change",
            workspace_bullets([
                format!("provider/model: {}:{} -> {}:{}", current.provider, current.model, target.provider, target.model),
                format!("working dir: {} -> {}", current.working_dir, target.working_dir),
                format!("tail preview: {} -> {}", checkpoint_tail_preview(current), checkpoint_tail_preview(target)),
            ]),
        )
        .footer(checkpoint_operator_guide())
        .render()
}

pub(crate) fn render_rollback_preview(
    current: &SessionCheckpointPayload,
    target: &SessionCheckpointPayload,
    target_label: &str,
) -> String {
    WorkspaceText::new("Rollback preview")
        .subtitle(target_label.to_string())
        .field("Mutation", "none (preview only)")
        .field("Current messages", current.message_count.to_string())
        .field("Rollback messages", target.message_count.to_string())
        .field(
            "Conflict severity",
            restore_conflict_severity(current, target).to_string(),
        )
        .section(
            "Rollback checks",
            workspace_bullets(render_restore_conflict_summary(current, target)),
        )
        .footer(checkpoint_operator_guide())
        .render()
}

pub(crate) fn render_rewind_safety_summary(
    current: &SessionCheckpointPayload,
    target: &SessionCheckpointPayload,
    target_label: &str,
    anchor_path: Option<&Path>,
) -> String {
    let message_delta = current.message_count as isize - target.message_count as isize;
    WorkspaceText::new("Rewind safety summary")
        .subtitle(target_label.to_string())
        .field("Mutation", "none (preview only)")
        .field("Message delta", format!("{}", message_delta))
        .field(
            "Transcript anchor",
            anchor_path
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".to_string()),
        )
        .section(
            "Safety checks",
            workspace_bullets([
                format!(
                    "provider/model drift: {}:{} -> {}:{}",
                    current.provider, current.model, target.provider, target.model
                ),
                format!(
                    "working dir drift: {} -> {}",
                    current.working_dir, target.working_dir
                ),
                format!(
                    "tail change: {} -> {}",
                    checkpoint_tail_preview(current),
                    checkpoint_tail_preview(target)
                ),
                "restore would replace current visible conversation state if enabled later".to_string(),
            ]),
        )
        .footer(checkpoint_operator_guide())
        .render()
}

pub(crate) fn render_branch_merge_preview(
    preview: &BranchMergePreview,
    state_path: &Path,
) -> String {
    WorkspaceText::new("Branch merge preview")
        .subtitle(preview.branch_label.clone())
        .field("Current", preview.current_label.clone())
        .field("Common prefix", preview.common_prefix_messages.to_string())
        .field("Branch only", preview.branch_only_messages.to_string())
        .field("Current only", preview.current_only_messages.to_string())
        .field("Merged count", preview.merged_message_count.to_string())
        .field("State artifact", state_path.display().to_string())
        .section("Conflicts", workspace_bullets(preview.conflicts.clone()))
        .footer(checkpoint_operator_guide())
        .render()
}

pub(crate) fn checkpoint_operator_guide() -> &'static str {
    "Operator guide: save with `/checkpoint save [label]`, branch with `/checkpoint branch save <name>`, inspect with `/checkpoint latest`, compare with `/checkpoint diff latest latest-1`, and preview rewind/restore via `/checkpoint rewind latest` or `/checkpoint restore-dry-run latest`. Merge a branch with `/checkpoint branch merge <target>` after previewing it with `merge-dry-run`."
}

pub(crate) fn render_restore_doctor(project_root: &Path) -> String {
    let latest_checkpoint = checkpoint_inventory(project_root, 1).into_iter().next();
    let latest_branch = branch_inventory(project_root, 1).into_iter().next();
    let latest_rewind = rewind_anchor_inventory(project_root, 1).into_iter().next();
    let latest_merge = latest_artifact_by_suffix(&checkpoint_dir(project_root), "branch-merge.md");
    let latest_rollback = rollback_anchor_inventory(project_root, 1).into_iter().next();
    WorkspaceText::new("Restore control doctor")
        .subtitle(project_root.display().to_string())
        .field(
            "Latest checkpoint",
            latest_checkpoint
                .as_ref()
                .map(|entry| entry.summary_path.display().to_string())
                .unwrap_or_else(|| "none".to_string()),
        )
        .field(
            "Latest branch",
            latest_branch
                .as_ref()
                .map(|entry| entry.summary_path.display().to_string())
                .unwrap_or_else(|| "none".to_string()),
        )
        .field(
            "Latest rewind",
            latest_rewind
                .as_ref()
                .map(|entry| entry.summary_path.display().to_string())
                .unwrap_or_else(|| "none".to_string()),
        )
        .field(
            "Latest merge preview",
            latest_merge
                .as_ref()
                .map(|path: &PathBuf| path.display().to_string())
                .unwrap_or_else(|| "none".to_string()),
        )
        .field(
            "Latest rollback",
            latest_rollback
                .as_ref()
                .map(|entry| entry.summary_path.display().to_string())
                .unwrap_or_else(|| "none".to_string()),
        )
        .section(
            "Checks",
            workspace_bullets([
                if latest_checkpoint.is_some() {
                    "[ok] checkpoint artifact available".to_string()
                } else {
                    "[--] checkpoint artifact unavailable".to_string()
                },
                if latest_branch.is_some() {
                    "[ok] branch artifact available".to_string()
                } else {
                    "[--] branch artifact unavailable".to_string()
                },
                if latest_rewind.is_some() {
                    "[ok] rewind anchor available".to_string()
                } else {
                    "[--] rewind anchor unavailable".to_string()
                },
                if latest_rollback.is_some() {
                    "[ok] rollback anchor available".to_string()
                } else {
                    "[--] rollback anchor unavailable".to_string()
                },
            ]),
        )
        .footer(checkpoint_operator_guide())
        .render()
}

pub(crate) fn build_current_checkpoint_payload(
    project_root: &Path,
    session_id: &str,
    provider: &str,
    model: &str,
    label: &str,
    chat_entries: &[ChatEntry],
    engine_messages: &[Message],
) -> SessionCheckpointPayload {
    build_checkpoint_payload(
        project_root,
        session_id,
        provider,
        model,
        label,
        chat_entries,
        engine_messages,
    )
}

pub(crate) fn load_checkpoint_payload(path: &Path) -> anyhow::Result<SessionCheckpointPayload> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

pub(crate) fn load_branch_merge_preview(path: &Path) -> anyhow::Result<BranchMergePreview> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

pub(crate) fn checkpoint_dir(project_root: &Path) -> PathBuf {
    project_root.join(".yode").join("checkpoints")
}

pub(crate) fn write_branch_snapshot(
    project_root: &Path,
    branch_name: &str,
    source: &SessionCheckpointPayload,
    source_summary: Option<&Path>,
) -> anyhow::Result<SessionCheckpointArtifactSet> {
    let dir = checkpoint_dir(project_root);
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let slug = checkpoint_slug(branch_name);
    let base = format!("{}-{}", stamp, slug);
    let state_path = dir.join(format!("{}-branch-state.json", base));
    let summary_path = dir.join(format!("{}-branch.md", base));
    let mut payload = source.clone();
    payload.kind = "session_branch".to_string();
    payload.label = format!("branch {}", branch_name);
    payload.created_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    payload.lineage.branch_name = Some(branch_name.to_string());
    payload.lineage.source_kind = Some(source.kind.clone());
    payload.lineage.source_label = Some(source.label.clone());
    payload.lineage.source_summary_artifact =
        source_summary.map(|path| path.display().to_string());
    std::fs::write(&state_path, serde_json::to_string_pretty(&payload)?)?;
    std::fs::write(&summary_path, render_checkpoint_summary(&payload, &state_path, Some(branch_name)))?;
    Ok(SessionCheckpointArtifactSet {
        summary_path,
        state_path,
    })
}

pub(crate) fn write_rewind_anchor(
    project_root: &Path,
    current: &SessionCheckpointPayload,
    target: &SessionCheckpointPayload,
    target_label: &str,
) -> anyhow::Result<SessionCheckpointArtifactSet> {
    let dir = checkpoint_dir(project_root);
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let slug = checkpoint_slug(target_label);
    let base = format!("{}-{}", stamp, slug);
    let state_path = dir.join(format!("{}-rewind-anchor-state.json", base));
    let summary_path = dir.join(format!("{}-rewind-anchor.md", base));
    let mut payload = current.clone();
    payload.kind = "rewind_anchor".to_string();
    payload.label = format!("rewind anchor -> {}", target_label);
    payload.created_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    payload.lineage.rewind_target_label = Some(target.label.clone());
    payload.lineage.source_kind = Some(target.kind.clone());
    payload.lineage.source_label = Some(target.label.clone());
    payload.lineage.transcript_anchor = current.artifacts.latest_transcript.clone();
    std::fs::write(&state_path, serde_json::to_string_pretty(&payload)?)?;
    std::fs::write(&summary_path, render_checkpoint_summary(&payload, &state_path, None))?;
    Ok(SessionCheckpointArtifactSet {
        summary_path,
        state_path,
    })
}

pub(crate) fn write_branch_merge_preview(
    project_root: &Path,
    current: &SessionCheckpointPayload,
    branch: &SessionCheckpointPayload,
    branch_label: &str,
) -> anyhow::Result<SessionCheckpointArtifactSet> {
    let dir = checkpoint_dir(project_root);
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let slug = checkpoint_slug(branch_label);
    let base = format!("{}-{}", stamp, slug);
    let state_path = dir.join(format!("{}-branch-merge-state.json", base));
    let summary_path = dir.join(format!("{}-branch-merge.md", base));
    let preview = build_branch_merge_preview(current, branch, branch_label);
    std::fs::write(&state_path, serde_json::to_string_pretty(&preview)?)?;
    std::fs::write(&summary_path, render_branch_merge_preview(&preview, &state_path))?;
    Ok(SessionCheckpointArtifactSet {
        summary_path,
        state_path,
    })
}

pub(crate) fn write_branch_merge_execution_artifact(
    project_root: &Path,
    current: &SessionCheckpointPayload,
    branch: &SessionCheckpointPayload,
    branch_label: &str,
) -> anyhow::Result<SessionCheckpointArtifactSet> {
    let dir = checkpoint_dir(project_root);
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let slug = checkpoint_slug(branch_label);
    let base = format!("{}-{}", stamp, slug);
    let state_path = dir.join(format!("{}-branch-merge-execution-state.json", base));
    let summary_path = dir.join(format!("{}-branch-merge-execution.md", base));
    let payload = build_branch_merge_execution_payload(current, branch, branch_label);
    std::fs::write(&state_path, serde_json::to_string_pretty(&payload)?)?;
    let body = format!(
        "# Branch Merge Execution\n\n- Branch: {}\n- Current: {}\n- Merged messages: {}\n- Current tail: {}\n- Branch tail: {}\n- Timestamp: {}\n- State artifact: {}\n",
        payload.branch_label,
        payload.current_label,
        payload.merged_message_count,
        payload.current_tail_count,
        payload.branch_tail_count,
        payload.merged_at,
        state_path.display(),
    );
    std::fs::write(&summary_path, body)?;
    Ok(SessionCheckpointArtifactSet {
        summary_path,
        state_path,
    })
}

pub(crate) fn write_restore_rollback_anchor(
    project_root: &Path,
    current: &SessionCheckpointPayload,
    target_label: &str,
) -> anyhow::Result<SessionCheckpointArtifactSet> {
    write_rollback_anchor(project_root, current, "restore", target_label)
}

pub(crate) fn write_merge_rollback_anchor(
    project_root: &Path,
    current: &SessionCheckpointPayload,
    target_label: &str,
) -> anyhow::Result<SessionCheckpointArtifactSet> {
    write_rollback_anchor(project_root, current, "merge", target_label)
}

fn recent_snapshot_summary_paths(project_root: &Path, suffixes: &[&str], limit: usize) -> Vec<PathBuf> {
    let mut entries = std::fs::read_dir(checkpoint_dir(project_root))
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().and_then(|ext| ext.to_str()) == Some("md")
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| suffixes.iter().any(|suffix| name.ends_with(suffix)))
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    entries.into_iter().take(limit).collect()
}

fn build_checkpoint_payload(
    project_root: &Path,
    session_id: &str,
    provider: &str,
    model: &str,
    label: &str,
    chat_entries: &[ChatEntry],
    engine_messages: &[Message],
) -> SessionCheckpointPayload {
    let messages = chat_entries
        .iter()
        .enumerate()
        .map(|(index, entry)| CheckpointMessage {
            index: index + 1,
            role: checkpoint_role_label(&entry.role),
            content: entry.content.clone(),
            reasoning: entry.reasoning.clone(),
            tool_metadata: entry.tool_metadata.clone(),
            tool_error_type: entry.tool_error_type.clone(),
        })
        .collect::<Vec<_>>();
    let mut role_counts = BTreeMap::new();
    for message in &messages {
        *role_counts.entry(message.role.clone()).or_insert(0) += 1;
    }

    SessionCheckpointPayload {
        schema_version: 1,
        kind: "session_checkpoint".to_string(),
        label: if label.trim().is_empty() {
            "manual checkpoint".to_string()
        } else {
            label.trim().to_string()
        },
        session_id: session_id.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        working_dir: project_root.display().to_string(),
        created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        message_count: messages.len(),
        role_counts,
        artifacts: CheckpointArtifacts {
            latest_review: latest_markdown_file(&project_root.join(".yode").join("reviews"))
                .map(|path| path.display().to_string()),
            latest_transcript: latest_markdown_file(&project_root.join(".yode").join("transcripts"))
                .map(|path| path.display().to_string()),
            latest_workflow: latest_workflow_execution_artifact(project_root)
                .map(|path| path.display().to_string()),
            latest_coordinate: latest_coordinator_artifact(project_root)
                .map(|path| path.display().to_string()),
            latest_orchestration: latest_runtime_orchestration_artifact(project_root)
                .map(|path| path.display().to_string()),
        },
        lineage: CheckpointLineage::default(),
        engine_messages: engine_messages.to_vec(),
        messages,
    }
}

fn build_branch_merge_preview(
    current: &SessionCheckpointPayload,
    branch: &SessionCheckpointPayload,
    branch_label: &str,
) -> BranchMergePreview {
    let common_prefix_messages = current
        .messages
        .iter()
        .zip(branch.messages.iter())
        .take_while(|(left, right)| left.role == right.role && left.content == right.content)
        .count();
    let branch_only_messages = branch.messages.len().saturating_sub(common_prefix_messages);
    let current_only_messages = current.messages.len().saturating_sub(common_prefix_messages);
    let merged_message_count = common_prefix_messages + branch_only_messages + current_only_messages;
    let conflicts = render_restore_conflict_summary(current, branch);
    BranchMergePreview {
        kind: "branch_merge_preview".to_string(),
        branch_label: branch_label.to_string(),
        current_label: current.label.clone(),
        common_prefix_messages,
        branch_only_messages,
        current_only_messages,
        merged_message_count,
        conflicts,
        generated_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    }
}

fn restore_conflict_severity(
    current: &SessionCheckpointPayload,
    target: &SessionCheckpointPayload,
) -> &'static str {
    let conflicts = render_restore_conflict_summary(current, target);
    if conflicts.iter().any(|line| line.contains("provider/model")) {
        "high"
    } else if conflicts.iter().any(|line| line.contains("working dir")) {
        "warn"
    } else if conflicts.iter().any(|line| line.contains("tail divergence")) {
        "medium"
    } else {
        "low"
    }
}

fn build_branch_merge_execution_payload(
    current: &SessionCheckpointPayload,
    branch: &SessionCheckpointPayload,
    branch_label: &str,
) -> BranchMergeExecutionPayload {
    let common_prefix_messages = current
        .messages
        .iter()
        .zip(branch.messages.iter())
        .take_while(|(left, right)| left.role == right.role && left.content == right.content)
        .count();
    let branch_tail_count = branch.messages.len().saturating_sub(common_prefix_messages);
    let current_tail_count = current.messages.len().saturating_sub(common_prefix_messages);
    BranchMergeExecutionPayload {
        kind: "branch_merge_execution".to_string(),
        branch_label: branch_label.to_string(),
        current_label: current.label.clone(),
        merged_message_count: common_prefix_messages + current_tail_count + branch_tail_count,
        current_tail_count,
        branch_tail_count,
        merged_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    }
}

fn render_restore_conflict_summary(
    current: &SessionCheckpointPayload,
    target: &SessionCheckpointPayload,
) -> Vec<String> {
    let mut conflicts = Vec::new();
    if current.provider != target.provider || current.model != target.model {
        conflicts.push(format!(
            "provider/model drift: {}:{} -> {}:{}",
            current.provider, current.model, target.provider, target.model
        ));
    }
    if current.working_dir != target.working_dir {
        conflicts.push(format!(
            "working dir drift: {} -> {}",
            current.working_dir, target.working_dir
        ));
    }
    if checkpoint_tail_preview(current) != checkpoint_tail_preview(target) {
        conflicts.push(format!(
            "tail divergence: {} -> {}",
            checkpoint_tail_preview(current),
            checkpoint_tail_preview(target)
        ));
    }
    if conflicts.is_empty() {
        conflicts.push("no structural conflicts detected".to_string());
    }
    conflicts
}

fn write_rollback_anchor(
    project_root: &Path,
    current: &SessionCheckpointPayload,
    kind_label: &str,
    target_label: &str,
) -> anyhow::Result<SessionCheckpointArtifactSet> {
    let dir = checkpoint_dir(project_root);
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let slug = checkpoint_slug(target_label);
    let base = format!("{}-{}", stamp, slug);
    let state_suffix = format!("{}-rollback-state.json", kind_label);
    let summary_suffix = format!("{}-rollback.md", kind_label);
    let state_path = dir.join(format!("{}-{}", base, state_suffix));
    let summary_path = dir.join(format!("{}-{}", base, summary_suffix));
    let mut payload = current.clone();
    payload.kind = format!("{}_rollback_anchor", kind_label);
    payload.label = format!("{} rollback -> {}", kind_label, target_label);
    payload.created_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    payload.lineage.source_kind = Some(current.kind.clone());
    payload.lineage.source_label = Some(current.label.clone());
    std::fs::write(&state_path, serde_json::to_string_pretty(&payload)?)?;
    std::fs::write(&summary_path, render_checkpoint_summary(&payload, &state_path, None))?;
    Ok(SessionCheckpointArtifactSet {
        summary_path,
        state_path,
    })
}

pub(crate) fn checkpoint_restore_messages(payload: &SessionCheckpointPayload) -> Vec<Message> {
    if !payload.engine_messages.is_empty() {
        return payload.engine_messages.clone();
    }

    payload
        .messages
        .iter()
        .filter_map(|message| checkpoint_message_to_engine_message(message))
        .collect()
}

pub(crate) fn checkpoint_restore_chat_entries(payload: &SessionCheckpointPayload) -> Vec<ChatEntry> {
    payload
        .messages
        .iter()
        .map(|message| {
            let role = checkpoint_role_to_chat_role(&message.role);
            let mut entry = ChatEntry::new(role, message.content.clone());
            entry.reasoning = message.reasoning.clone();
            entry.tool_metadata = message.tool_metadata.clone();
            entry.tool_error_type = message.tool_error_type.clone();
            entry
        })
        .collect()
}

pub(crate) fn merge_checkpoint_payloads(
    current: &SessionCheckpointPayload,
    branch: &SessionCheckpointPayload,
) -> (Vec<Message>, Vec<ChatEntry>) {
    let common_prefix_messages = current
        .messages
        .iter()
        .zip(branch.messages.iter())
        .take_while(|(left, right)| left.role == right.role && left.content == right.content)
        .count();

    let mut merged_engine = checkpoint_restore_messages(current);
    merged_engine.extend(
        checkpoint_restore_messages(branch)
            .into_iter()
            .skip(common_prefix_messages),
    );

    let mut merged_chat = checkpoint_restore_chat_entries(current);
    merged_chat.extend(
        checkpoint_restore_chat_entries(branch)
            .into_iter()
            .skip(common_prefix_messages),
    );

    (merged_engine, merged_chat)
}

fn render_checkpoint_summary(
    payload: &SessionCheckpointPayload,
    state_path: &Path,
    branch_name: Option<&str>,
) -> String {
    let mut lines = vec![
        match payload.kind.as_str() {
            "session_branch" => "# Session Branch".to_string(),
            "rewind_anchor" => "# Rewind Anchor".to_string(),
            _ => "# Session Checkpoint".to_string(),
        },
        String::new(),
        format!("- Kind: {}", payload.kind),
        format!("- Label: {}", payload.label),
        format!("- Session: {}", payload.session_id),
        format!("- Provider: {}", payload.provider),
        format!("- Model: {}", payload.model),
        format!("- Working dir: {}", payload.working_dir),
        format!("- Timestamp: {}", payload.created_at),
        format!("- Message count: {}", payload.message_count),
        format!("- State artifact: {}", state_path.display()),
        String::new(),
        "Lineage:".to_string(),
        format!(
            "- branch: {}",
            branch_name
                .or(payload.lineage.branch_name.as_deref())
                .unwrap_or("none")
        ),
        format!(
            "- source kind: {}",
            payload.lineage.source_kind.as_deref().unwrap_or("none")
        ),
        format!(
            "- source label: {}",
            payload.lineage.source_label.as_deref().unwrap_or("none")
        ),
        format!(
            "- source summary: {}",
            payload
                .lineage
                .source_summary_artifact
                .as_deref()
                .unwrap_or("none")
        ),
        format!(
            "- rewind target: {}",
            payload
                .lineage
                .rewind_target_label
                .as_deref()
                .unwrap_or("none")
        ),
        format!(
            "- transcript anchor: {}",
            payload
                .lineage
                .transcript_anchor
                .as_deref()
                .unwrap_or("none")
        ),
        String::new(),
        "Role counts:".to_string(),
    ];
    for (role, count) in &payload.role_counts {
        lines.push(format!("- {}: {}", role, count));
    }

    lines.push(String::new());
    lines.push("Artifacts:".to_string());
    for (label, value) in [
        ("review", payload.artifacts.latest_review.as_deref()),
        ("transcript", payload.artifacts.latest_transcript.as_deref()),
        ("workflow", payload.artifacts.latest_workflow.as_deref()),
        ("coordinate", payload.artifacts.latest_coordinate.as_deref()),
        ("orchestration", payload.artifacts.latest_orchestration.as_deref()),
    ] {
        lines.push(format!("- {}: {}", label, value.unwrap_or("none")));
    }

    lines.push(String::new());
    lines.push("Messages Preview:".to_string());
    for message in payload.messages.iter().rev().take(8).rev() {
        lines.push(format!(
            "- {}. {}: {}",
            message.index,
            message.role,
            truncate_preview(&message.content, 160)
        ));
    }
    lines.push(String::new());
    lines.push(checkpoint_operator_guide().to_string());
    lines.join("\n")
}

fn latest_markdown_file(dir: &Path) -> Option<PathBuf> {
    let mut entries = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    entries.into_iter().next()
}

fn summary_path_to_state_path(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    for (summary_suffix, state_suffix) in [
        ("checkpoint.md", "checkpoint-state.json"),
        ("branch.md", "branch-state.json"),
        ("rewind-anchor.md", "rewind-anchor-state.json"),
    ] {
        if let Some(prefix) = name.strip_suffix(summary_suffix) {
            return Some(path.with_file_name(format!("{}{}", prefix, state_suffix)));
        }
    }
    None
}

fn checkpoint_slug(label: &str) -> String {
    let slug = label
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "checkpoint".to_string()
    } else {
        slug.to_string()
    }
}

fn checkpoint_role_label(role: &ChatRole) -> String {
    match role {
        ChatRole::User => "user".to_string(),
        ChatRole::Assistant => "assistant".to_string(),
        ChatRole::ToolCall { name, .. } => format!("tool_call:{}", name),
        ChatRole::ToolResult { name, is_error, .. } => {
            if *is_error {
                format!("tool_result_error:{}", name)
            } else {
                format!("tool_result:{}", name)
            }
        }
        ChatRole::Error => "error".to_string(),
        ChatRole::System => "system".to_string(),
        ChatRole::SubAgentCall { description } => format!("subagent_call:{}", checkpoint_slug(description)),
        ChatRole::SubAgentToolCall { name } => format!("subagent_tool:{}", name),
        ChatRole::SubAgentResult => "subagent_result".to_string(),
        ChatRole::AskUser { id } => format!("ask_user:{}", id),
    }
}

fn checkpoint_role_to_chat_role(role: &str) -> ChatRole {
    if let Some(name) = role.strip_prefix("tool_call:") {
        return ChatRole::ToolCall {
            id: format!("restored-{}", checkpoint_slug(name)),
            name: name.to_string(),
        };
    }
    if let Some(name) = role.strip_prefix("tool_result_error:") {
        return ChatRole::ToolResult {
            id: format!("restored-{}", checkpoint_slug(name)),
            name: name.to_string(),
            is_error: true,
        };
    }
    if let Some(name) = role.strip_prefix("tool_result:") {
        return ChatRole::ToolResult {
            id: format!("restored-{}", checkpoint_slug(name)),
            name: name.to_string(),
            is_error: false,
        };
    }
    if let Some(name) = role.strip_prefix("subagent_call:") {
        return ChatRole::SubAgentCall {
            description: name.to_string(),
        };
    }
    if let Some(name) = role.strip_prefix("subagent_tool:") {
        return ChatRole::SubAgentToolCall {
            name: name.to_string(),
        };
    }
    if let Some(id) = role.strip_prefix("ask_user:") {
        return ChatRole::AskUser {
            id: id.to_string(),
        };
    }
    match role {
        "assistant" => ChatRole::Assistant,
        "system" => ChatRole::System,
        "error" => ChatRole::Error,
        "subagent_result" => ChatRole::SubAgentResult,
        _ => ChatRole::User,
    }
}

fn checkpoint_message_to_engine_message(message: &CheckpointMessage) -> Option<Message> {
    if message.role.starts_with("tool_call:") {
        return Some(Message {
            role: Role::Assistant,
            content: Some(message.content.clone()),
            content_blocks: Vec::new(),
            reasoning: message.reasoning.clone(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            images: Vec::new(),
        }
        .normalized());
    }

    if message.role.starts_with("tool_result") {
        return Some(Message::tool_result(
            format!("restored-{}", message.index),
            message.content.clone(),
        ));
    }

    let role = match message.role.as_str() {
        "user" => Role::User,
        "assistant" => Role::Assistant,
        "system" => Role::System,
        "error" => Role::System,
        "subagent_result" => Role::Assistant,
        other if other.starts_with("subagent_call:") => Role::Assistant,
        other if other.starts_with("subagent_tool:") => Role::Tool,
        other if other.starts_with("ask_user:") => Role::System,
        _ => return None,
    };

    Some(
        Message {
            role,
            content: Some(message.content.clone()),
            content_blocks: Vec::new(),
            reasoning: message.reasoning.clone(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            images: Vec::new(),
        }
        .normalized(),
    )
}

fn checkpoint_tail_preview(payload: &SessionCheckpointPayload) -> String {
    payload
        .messages
        .last()
        .map(|message| format!("{}: {}", message.role, truncate_preview(&message.content, 100)))
        .unwrap_or_else(|| "none".to_string())
}

fn truncate_preview(text: &str, max_chars: usize) -> String {
    let squashed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if squashed.chars().count() <= max_chars {
        squashed
    } else {
        format!("{}...", squashed.chars().take(max_chars).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use crate::app::{ChatEntry, ChatRole};
    use yode_llm::types::{Message, Role};

    use super::{
        branch_inventory, build_current_checkpoint_payload, checkpoint_completion_targets,
        checkpoint_inventory, checkpoint_operator_guide, checkpoint_restore_chat_entries,
        checkpoint_restore_messages, render_checkpoint_diff, render_restore_dry_run,
        render_rewind_safety_summary, resolve_branch_target, resolve_checkpoint_target,
        rewind_anchor_inventory, write_branch_snapshot, write_rewind_anchor,
        write_session_checkpoint,
    };

    #[test]
    fn writes_and_resolves_checkpoint_artifacts() {
        let dir = std::env::temp_dir().join(format!("yode-checkpoint-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let chat = vec![
            ChatEntry::new(ChatRole::User, "hello".to_string()),
            ChatEntry::new(ChatRole::Assistant, "world".to_string()),
        ];
        let artifacts = write_session_checkpoint(
            &dir,
            "session-1234",
            "anthropic",
            "claude",
            "demo",
            &chat,
            &[Message::user("hello"), Message::assistant("world")],
        )
        .unwrap();
        assert!(artifacts.summary_path.exists());
        assert!(artifacts.state_path.exists());
        let entry = resolve_checkpoint_target(&dir, "latest").unwrap();
        assert_eq!(entry.payload.message_count, 2);
        assert_eq!(checkpoint_inventory(&dir, 4).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn diff_and_restore_dry_run_render_workspace_text() {
        let dir = std::env::temp_dir().join(format!("yode-checkpoint-diff-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let left = build_current_checkpoint_payload(
            &dir,
            "session-a",
            "anthropic",
            "claude",
            "left",
            &[ChatEntry::new(ChatRole::User, "hello".to_string())],
            &[Message::user("hello")],
        );
        let right = build_current_checkpoint_payload(
            &dir,
            "session-a",
            "openai",
            "gpt",
            "right",
            &[
                ChatEntry::new(ChatRole::User, "hello".to_string()),
                ChatEntry::new(ChatRole::Assistant, "world".to_string()),
            ],
            &[Message::user("hello"), Message::assistant("world")],
        );
        let diff = render_checkpoint_diff(&left, &right, "left", "right");
        assert!(diff.contains("Session checkpoint diff"));
        let dry_run = render_restore_dry_run(&left, &right, "right");
        assert!(dry_run.contains("Mutation"));
        assert!(checkpoint_operator_guide().contains("/checkpoint save"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn completion_targets_and_index_resolution_work() {
        let dir = std::env::temp_dir().join(format!("yode-checkpoint-complete-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let chat = vec![ChatEntry::new(ChatRole::User, "hello".to_string())];
        let _ = write_session_checkpoint(
            &dir,
            "session-1234",
            "anthropic",
            "claude",
            "demo",
            &chat,
            &[Message::user("hello")],
        )
        .unwrap();
        let targets = checkpoint_completion_targets(dir.to_str().unwrap());
        assert!(targets.iter().any(|target| target == "latest"));
        assert!(resolve_checkpoint_target(&dir, "1").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn branch_and_rewind_artifacts_render_and_resolve() {
        let dir = std::env::temp_dir().join(format!("yode-branch-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let base = build_current_checkpoint_payload(
            &dir,
            "session-a",
            "anthropic",
            "claude",
            "base",
            &[ChatEntry::new(ChatRole::User, "hello".to_string())],
            &[Message::user("hello")],
        );
        let branch = write_branch_snapshot(&dir, "feature-a", &base, None).unwrap();
        let rewind = write_rewind_anchor(&dir, &base, &base, "latest").unwrap();
        assert!(branch.summary_path.exists());
        assert!(rewind.summary_path.exists());
        assert_eq!(branch_inventory(&dir, 8).len(), 1);
        assert_eq!(rewind_anchor_inventory(&dir, 8).len(), 1);
        assert!(resolve_branch_target(&dir, "latest").is_some());
        let summary = render_rewind_safety_summary(&base, &base, "latest", Some(&rewind.summary_path));
        assert!(summary.contains("Rewind safety summary"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restore_helpers_rebuild_engine_messages_and_chat_entries() {
        let dir = std::env::temp_dir().join(format!("yode-checkpoint-restore-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let chat = vec![
            ChatEntry::new(ChatRole::User, "hello".to_string()),
            ChatEntry::new(ChatRole::Assistant, "world".to_string()),
        ];
        let payload = build_current_checkpoint_payload(
            &dir,
            "session-a",
            "anthropic",
            "claude",
            "restore",
            &chat,
            &[Message::user("hello"), Message::assistant("world")],
        );
        let restored_messages = checkpoint_restore_messages(&payload);
        let restored_chat = checkpoint_restore_chat_entries(&payload);
        assert_eq!(restored_messages.len(), 2);
        assert!(matches!(restored_messages[0].role, Role::User));
        assert_eq!(restored_chat.len(), 2);
        assert!(matches!(restored_chat[0].role, ChatRole::User));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
