use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::app::{ChatEntry, ChatRole};
use crate::commands::artifact_nav::{
    artifact_freshness_badge, latest_coordinator_artifact, latest_runtime_orchestration_artifact,
    latest_workflow_execution_artifact,
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
    pub messages: Vec<CheckpointMessage>,
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
) -> anyhow::Result<SessionCheckpointArtifactSet> {
    let dir = checkpoint_dir(project_root);
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let short_session = session_id.chars().take(8).collect::<String>();
    let slug = checkpoint_slug(label);
    let base = format!("{}-{}-{}", stamp, short_session, slug);
    let state_path = dir.join(format!("{}-checkpoint-state.json", base));
    let summary_path = dir.join(format!("{}-checkpoint.md", base));

    let payload = build_checkpoint_payload(project_root, session_id, provider, model, label, chat_entries);
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
        "rewind-anchor".to_string(),
        "rewind-anchor list".to_string(),
        "rewind-anchor latest".to_string(),
        "rewind-anchor save latest".to_string(),
        "rewind latest".to_string(),
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

pub(crate) fn checkpoint_operator_guide() -> &'static str {
    "Operator guide: save with `/checkpoint save [label]`, branch with `/checkpoint branch save <name>`, inspect with `/checkpoint latest`, compare with `/checkpoint diff latest latest-1`, and preview rewind/restore via `/checkpoint rewind latest` or `/checkpoint restore-dry-run latest`."
}

pub(crate) fn build_current_checkpoint_payload(
    project_root: &Path,
    session_id: &str,
    provider: &str,
    model: &str,
    label: &str,
    chat_entries: &[ChatEntry],
) -> SessionCheckpointPayload {
    build_checkpoint_payload(project_root, session_id, provider, model, label, chat_entries)
}

pub(crate) fn load_checkpoint_payload(path: &Path) -> anyhow::Result<SessionCheckpointPayload> {
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
        messages,
    }
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

    use super::{
        branch_inventory, build_current_checkpoint_payload, checkpoint_completion_targets,
        checkpoint_inventory, checkpoint_operator_guide, render_checkpoint_diff,
        render_restore_dry_run, render_rewind_safety_summary, resolve_branch_target,
        resolve_checkpoint_target, rewind_anchor_inventory, write_branch_snapshot,
        write_rewind_anchor, write_session_checkpoint,
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
}
