use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use yode_tools::registry::ToolPoolSnapshot;

const TOOL_ARTIFACTS_DIR: &str = ".yode/tools";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolResultTruncationView {
    pub reason: String,
    pub original_bytes: usize,
    pub kept_bytes: usize,
    pub omitted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolRuntimeCallView {
    pub call_id: String,
    pub tool_name: String,
    pub started_at: Option<String>,
    pub duration_ms: u64,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub progress_updates: u32,
    pub success: bool,
    pub error_type: Option<String>,
    pub parallel_batch: Option<u32>,
    pub truncation: Option<ToolResultTruncationView>,
    pub repeated_failure_count: u32,
    pub metadata_summary: Option<String>,
    pub diff_preview: Option<String>,
    pub output_preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolTurnArtifact {
    pub turn_index: u64,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub total_calls: u32,
    pub success_count: u32,
    pub failed_count: u32,
    pub total_output_bytes: usize,
    pub truncated_results: u32,
    pub progress_events: u32,
    pub parallel_batches: u32,
    pub parallel_calls: u32,
    pub max_parallel_batch_size: usize,
    pub budget_notice_emitted: bool,
    pub budget_warning_emitted: bool,
    pub last_budget_warning: Option<String>,
    pub latest_repeated_failure: Option<String>,
    pub error_type_counts: BTreeMap<String, u32>,
    pub tool_pool: Option<ToolPoolArtifactView>,
    pub calls: Vec<ToolRuntimeCallView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolPoolArtifactView {
    pub permission_mode: String,
    pub tool_search_enabled: bool,
    pub tool_search_reason: Option<String>,
    pub active_visible_count: usize,
    pub active_hidden_count: usize,
    pub deferred_visible_count: usize,
    pub deferred_hidden_count: usize,
    pub confirm_count: usize,
    pub deny_count: usize,
    pub activation_count: usize,
    pub last_activated_tool: Option<String>,
    pub hidden_tools: Vec<String>,
    pub visible_deferred_tools: Vec<String>,
}

impl ToolPoolArtifactView {
    pub fn from_snapshot(
        snapshot: &ToolPoolSnapshot,
        activation_count: usize,
        last_activated_tool: Option<String>,
    ) -> Self {
        Self {
            permission_mode: snapshot.permission_mode.clone(),
            tool_search_enabled: snapshot.tool_search_enabled,
            tool_search_reason: snapshot.tool_search_reason.clone(),
            active_visible_count: snapshot.visible_active_count(),
            active_hidden_count: snapshot.hidden_active_count(),
            deferred_visible_count: snapshot.visible_deferred_count(),
            deferred_hidden_count: snapshot.hidden_deferred_count(),
            confirm_count: snapshot.confirm_count(),
            deny_count: snapshot.deny_count(),
            activation_count,
            last_activated_tool,
            hidden_tools: snapshot
                .hidden_tool_names()
                .into_iter()
                .map(str::to_string)
                .collect(),
            visible_deferred_tools: snapshot
                .visible_deferred_tool_names()
                .into_iter()
                .map(str::to_string)
                .collect(),
        }
    }
}

pub fn tool_artifacts_dir(project_root: &Path) -> PathBuf {
    project_root.join(TOOL_ARTIFACTS_DIR)
}

pub fn write_tool_turn_artifact(
    project_root: &Path,
    session_id: &str,
    artifact: &ToolTurnArtifact,
) -> Result<PathBuf> {
    let dir = tool_artifacts_dir(project_root);
    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create tool artifact dir: {}", dir.display()))?;

    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let short_session: String = session_id.chars().take(8).collect();
    let path = dir.join(format!(
        "{}-tools-turn-{:04}-{}.md",
        short_session, artifact.turn_index, timestamp
    ));

    fs::write(&path, render_tool_turn_artifact(artifact))
        .with_context(|| format!("Failed to write tool artifact file: {}", path.display()))?;

    Ok(path)
}

pub fn render_tool_turn_artifact(artifact: &ToolTurnArtifact) -> String {
    let mut output = String::new();
    output.push_str("# Tool Turn Artifact\n\n");
    output.push_str(&format!("- Turn: {}\n", artifact.turn_index));
    output.push_str(&format!(
        "- Started at: {}\n",
        artifact.started_at.as_deref().unwrap_or("unknown")
    ));
    output.push_str(&format!(
        "- Completed at: {}\n",
        artifact.completed_at.as_deref().unwrap_or("unknown")
    ));
    output.push_str(&format!("- Total calls: {}\n", artifact.total_calls));
    output.push_str(&format!("- Success: {}\n", artifact.success_count));
    output.push_str(&format!("- Failed: {}\n", artifact.failed_count));
    output.push_str(&format!(
        "- Output bytes: {}\n",
        artifact.total_output_bytes
    ));
    output.push_str(&format!(
        "- Truncated results: {}\n",
        artifact.truncated_results
    ));
    output.push_str(&format!(
        "- Progress events: {}\n",
        artifact.progress_events
    ));
    output.push_str(&format!(
        "- Parallel batches: {}\n",
        artifact.parallel_batches
    ));
    output.push_str(&format!("- Parallel calls: {}\n", artifact.parallel_calls));
    output.push_str(&format!(
        "- Max parallel batch: {}\n",
        artifact.max_parallel_batch_size
    ));
    output.push_str(&format!(
        "- Budget notice emitted: {}\n",
        if artifact.budget_notice_emitted {
            "yes"
        } else {
            "no"
        }
    ));
    output.push_str(&format!(
        "- Budget warning emitted: {}\n",
        if artifact.budget_warning_emitted {
            "yes"
        } else {
            "no"
        }
    ));
    if let Some(warning) = artifact.last_budget_warning.as_deref() {
        output.push_str(&format!("- Last budget warning: {}\n", warning));
    }
    if let Some(pattern) = artifact.latest_repeated_failure.as_deref() {
        output.push_str(&format!("- Latest repeated failure: {}\n", pattern));
    }
    if !artifact.error_type_counts.is_empty() {
        let counts = artifact
            .error_type_counts
            .iter()
            .map(|(kind, count)| format!("{}={}", kind, count))
            .collect::<Vec<_>>()
            .join(", ");
        output.push_str(&format!("- Error types: {}\n", counts));
    }
    if let Some(tool_pool) = &artifact.tool_pool {
        output.push_str("\n## Tool Pool\n");
        output.push_str(&format!(
            "- Permission mode: {}\n",
            tool_pool.permission_mode
        ));
        output.push_str(&format!(
            "- Tool search enabled: {}\n",
            tool_pool.tool_search_enabled
        ));
        output.push_str(&format!(
            "- Tool search reason: {}\n",
            tool_pool.tool_search_reason.as_deref().unwrap_or("none")
        ));
        output.push_str(&format!(
            "- Active visible: {}\n",
            tool_pool.active_visible_count
        ));
        output.push_str(&format!(
            "- Active hidden: {}\n",
            tool_pool.active_hidden_count
        ));
        output.push_str(&format!(
            "- Deferred visible: {}\n",
            tool_pool.deferred_visible_count
        ));
        output.push_str(&format!(
            "- Deferred hidden: {}\n",
            tool_pool.deferred_hidden_count
        ));
        output.push_str(&format!(
            "- Confirm-required: {}\n",
            tool_pool.confirm_count
        ));
        output.push_str(&format!("- Denied: {}\n", tool_pool.deny_count));
        output.push_str(&format!(
            "- Activations: {}\n",
            tool_pool.activation_count
        ));
        output.push_str(&format!(
            "- Last activated tool: {}\n",
            tool_pool.last_activated_tool.as_deref().unwrap_or("none")
        ));
        output.push_str(&format!(
            "- Hidden tools: {}\n",
            if tool_pool.hidden_tools.is_empty() {
                "none".to_string()
            } else {
                tool_pool.hidden_tools.join(", ")
            }
        ));
        output.push_str(&format!(
            "- Visible deferred tools: {}\n",
            if tool_pool.visible_deferred_tools.is_empty() {
                "none".to_string()
            } else {
                tool_pool.visible_deferred_tools.join(", ")
            }
        ));
    }

    output.push_str("\n## Calls\n");
    for call in &artifact.calls {
        output.push('\n');
        output.push_str(&format!(
            "### {} ({})\n\n",
            call.tool_name,
            if call.success { "ok" } else { "error" }
        ));
        output.push_str(&format!("- Call id: {}\n", call.call_id));
        output.push_str(&format!(
            "- Started at: {}\n",
            call.started_at.as_deref().unwrap_or("unknown")
        ));
        output.push_str(&format!("- Duration: {} ms\n", call.duration_ms));
        output.push_str(&format!("- Input bytes: {}\n", call.input_bytes));
        output.push_str(&format!("- Output bytes: {}\n", call.output_bytes));
        output.push_str(&format!("- Progress updates: {}\n", call.progress_updates));
        if let Some(err) = call.error_type.as_deref() {
            output.push_str(&format!("- Error type: {}\n", err));
        }
        if let Some(batch) = call.parallel_batch {
            output.push_str(&format!("- Parallel batch: {}\n", batch));
        }
        if let Some(truncation) = &call.truncation {
            output.push_str(&format!(
                "- Truncation: {} (original {}, kept {}, omitted {})\n",
                truncation.reason,
                truncation.original_bytes,
                truncation.kept_bytes,
                truncation.omitted_bytes
            ));
        }
        if call.repeated_failure_count > 1 {
            output.push_str(&format!(
                "- Repeated failure count: {}\n",
                call.repeated_failure_count
            ));
        }
        if let Some(summary) = call.metadata_summary.as_deref() {
            output.push_str(&format!("- Metadata: {}\n", summary));
        }
        if let Some(diff) = call.diff_preview.as_deref() {
            output.push_str("\n**Diff Preview**\n\n```diff\n");
            output.push_str(diff.trim_end());
            output.push_str("\n```\n");
        }
        if !call.output_preview.trim().is_empty() {
            output.push_str("\n**Output Preview**\n\n```text\n");
            output.push_str(call.output_preview.trim_end());
            output.push_str("\n```\n");
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use tempfile::tempdir;

    use super::{
        write_tool_turn_artifact, ToolPoolArtifactView, ToolRuntimeCallView, ToolTurnArtifact,
    };

    #[test]
    fn writes_tool_turn_artifact_file() {
        let temp = tempdir().unwrap();
        let artifact = ToolTurnArtifact {
            turn_index: 3,
            total_calls: 2,
            success_count: 1,
            failed_count: 1,
            tool_pool: Some(ToolPoolArtifactView {
                permission_mode: "default".into(),
                tool_search_enabled: true,
                deferred_visible_count: 2,
                ..ToolPoolArtifactView::default()
            }),
            calls: vec![ToolRuntimeCallView {
                call_id: "call-1".into(),
                tool_name: "bash".into(),
                output_preview: "preview".into(),
                ..ToolRuntimeCallView::default()
            }],
            ..ToolTurnArtifact::default()
        };

        let path = write_tool_turn_artifact(temp.path(), "session-1234", &artifact).unwrap();
        let content = std::fs::read_to_string(path).unwrap();

        assert!(content.contains("Tool Turn Artifact"));
        assert!(content.contains("Tool Pool"));
        assert!(content.contains("bash"));
        assert!(content.contains("preview"));
    }

    #[test]
    fn tool_turn_artifact_serializes_runtime_schema_fields() {
        let artifact = ToolTurnArtifact {
            turn_index: 7,
            total_calls: 1,
            progress_events: 3,
            latest_repeated_failure: Some("bash [Execution] x2".into()),
            error_type_counts: BTreeMap::from([("Execution".into(), 1)]),
            calls: vec![ToolRuntimeCallView {
                call_id: "call-1".into(),
                tool_name: "bash".into(),
                progress_updates: 3,
                repeated_failure_count: 2,
                output_preview: "tail -n 20".into(),
                ..ToolRuntimeCallView::default()
            }],
            ..ToolTurnArtifact::default()
        };

        let value = serde_json::to_value(&artifact).unwrap();
        assert_eq!(value["turn_index"].as_u64(), Some(7));
        assert_eq!(value["progress_events"].as_u64(), Some(3));
        assert_eq!(value["calls"][0]["progress_updates"].as_u64(), Some(3));
        assert_eq!(
            value["calls"][0]["repeated_failure_count"].as_u64(),
            Some(2)
        );
        assert_eq!(value["error_type_counts"]["Execution"].as_u64(), Some(1));
    }
}
