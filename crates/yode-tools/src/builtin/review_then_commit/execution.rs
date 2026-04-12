use anyhow::Result;
use serde_json::{json, Value};

use crate::builtin::git_commit::GitCommitTool;
use crate::builtin::review_common::{
    merge_review_metadata, persist_review_artifact, persist_review_status,
    render_review_artifact_message, render_review_then_commit_summary, review_findings_count,
    review_metadata_with_extra, review_output_has_findings,
};
use crate::tool::{SubAgentOptions, Tool, ToolContext, ToolErrorType, ToolResult};

pub(super) async fn execute_review_then_commit(
    params: Value,
    ctx: &ToolContext,
) -> Result<ToolResult> {
    let message = params
        .get("message")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("'message' parameter is required"))?;
    let focus = params
        .get("focus")
        .and_then(|value| value.as_str())
        .unwrap_or("current workspace changes");
    let instructions = params
        .get("instructions")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let allow_findings_commit = params
        .get("allow_findings_commit")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let runner = ctx
        .sub_agent_runner
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Sub-agent runner not available"))?;

    let review_prompt = format!(
        "You are a dedicated review agent for current workspace changes.\n\nFocus:\n{}\n\nInstructions:\n{}\n\nReview protocol:\n- Check repository state and changed files first.\n- Focus on bugs, regressions, risky assumptions, and missing tests.\n- Findings must come first, ordered by severity.\n- If no issues are found, say exactly 'No issues found.' on the first line.\n- Keep the review concise but specific.",
        focus,
        if instructions.is_empty() {
            "No extra instructions."
        } else {
            instructions
        }
    );

    let review_output = runner
        .run_sub_agent(
            review_prompt,
            SubAgentOptions {
                description: format!("review before commit {}", focus),
                subagent_type: Some("review".to_string()),
                model: None,
                run_in_background: false,
                isolation: None,
                cwd: None,
                allowed_tools: vec![
                    "read_file".to_string(),
                    "glob".to_string(),
                    "grep".to_string(),
                    "ls".to_string(),
                    "git_status".to_string(),
                    "git_diff".to_string(),
                    "git_log".to_string(),
                    "project_map".to_string(),
                    "test_runner".to_string(),
                    "bash".to_string(),
                ],
            },
        )
        .await?;

    let artifact_path = ctx
        .working_dir
        .as_deref()
        .and_then(|dir| {
            persist_review_artifact(dir, "pre-commit-review", focus, &review_output).ok()
        })
        .map(|path| path.display().to_string());
    let findings_count = review_findings_count(&review_output);
    if let (Some(dir), Some(path)) = (
        ctx.working_dir.as_deref(),
        artifact_path.as_deref().map(std::path::Path::new),
    ) {
        let _ = persist_review_status(dir, "pre-commit-review", focus, &review_output, Some(path));
    }

    if review_output_has_findings(&review_output) && !allow_findings_commit {
        let summary = render_review_then_commit_summary(&review_output, "skipped due to findings");
        return Ok(ToolResult {
            content: render_review_artifact_message(
                "Review detected findings. Commit aborted.",
                &summary,
                artifact_path.as_deref(),
            ),
            is_error: true,
            error_type: Some(ToolErrorType::Validation),
            recoverable: true,
            suggestion: Some(
                "Address the review findings first, or set allow_findings_commit=true if you intentionally want to override."
                    .to_string(),
            ),
            metadata: Some(review_metadata_with_extra(
                "pre-commit-review",
                focus,
                &review_output,
                artifact_path.as_deref(),
                json!({
                "review_output": review_output,
                "findings_count": findings_count,
                "review_artifact_path": artifact_path,
                "commit_skipped": true,
                }),
            )),
        });
    }

    let commit_result = GitCommitTool
        .execute(
            json!({
                "message": message,
                "files": params.get("files").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
                "all": params.get("all").cloned().unwrap_or_else(|| Value::Bool(false)),
            }),
            ctx,
        )
        .await?;

    let metadata = merge_review_metadata(
        commit_result.metadata.clone().unwrap_or_else(|| json!({})),
        review_metadata_with_extra(
            "pre-commit-review",
            focus,
            &review_output,
            artifact_path.as_deref(),
            json!({
                "review_output": review_output,
                "findings_count": findings_count,
                "review_artifact_path": artifact_path,
            }),
        ),
    );

    let summary = render_review_then_commit_summary(&review_output, &commit_result.content);
    Ok(ToolResult {
        content: render_review_artifact_message(
            "Review passed.",
            &summary,
            artifact_path.as_deref(),
        ),
        is_error: commit_result.is_error,
        error_type: commit_result.error_type,
        recoverable: commit_result.recoverable,
        suggestion: commit_result.suggestion,
        metadata: Some(metadata),
    })
}
