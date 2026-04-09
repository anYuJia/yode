use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

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
    pub calls: Vec<ToolRuntimeCallView>,
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
    use tempfile::tempdir;

    use super::{write_tool_turn_artifact, ToolRuntimeCallView, ToolTurnArtifact};

    #[test]
    fn writes_tool_turn_artifact_file() {
        let temp = tempdir().unwrap();
        let artifact = ToolTurnArtifact {
            turn_index: 3,
            total_calls: 2,
            success_count: 1,
            failed_count: 1,
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
        assert!(content.contains("bash"));
        assert!(content.contains("preview"));
    }
}
