use anyhow::Result;
use serde_json::{json, Value};

use crate::builtin::git_commit::GitCommitTool;
use crate::builtin::review_common::{
    persist_review_artifact, persist_review_status, review_findings_count,
    review_metadata_payload, review_output_has_findings,
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
        return Ok(ToolResult {
            content: format!(
                "Review detected findings. Commit aborted.\n\nReview output:\n{}\n\nReview artifact: {}",
                review_output,
                artifact_path.as_deref().unwrap_or("none")
            ),
            is_error: true,
            error_type: Some(ToolErrorType::Validation),
            recoverable: true,
            suggestion: Some(
                "Address the review findings first, or set allow_findings_commit=true if you intentionally want to override."
                    .to_string(),
            ),
            metadata: Some(merge_review_metadata(
                review_metadata_payload("pre-commit-review", focus, &review_output, artifact_path.as_deref()),
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

    let mut metadata = commit_result.metadata.clone().unwrap_or_else(|| json!({}));
    if let Some(object) = metadata.as_object_mut() {
        object.insert("review_output".to_string(), json!(review_output));
        object.insert("findings_count".to_string(), json!(findings_count));
        object.insert("review_artifact_path".to_string(), json!(artifact_path));
        object.insert(
            "review_artifact".to_string(),
            review_metadata_payload("pre-commit-review", focus, &review_output, artifact_path.as_deref())["review_artifact"].clone(),
        );
    }

    Ok(ToolResult {
        content: format!(
            "Review passed.\n\n{}\n\nReview artifact: {}",
            commit_result.content,
            artifact_path.as_deref().unwrap_or("none")
        ),
        is_error: commit_result.is_error,
        error_type: commit_result.error_type,
        recoverable: commit_result.recoverable,
        suggestion: commit_result.suggestion,
        metadata: Some(metadata),
    })
}

fn merge_review_metadata(mut base: Value, extra: Value) -> Value {
    if let (Some(base_object), Some(extra_object)) = (base.as_object_mut(), extra.as_object()) {
        for (key, value) in extra_object {
            base_object.insert(key.clone(), value.clone());
        }
    }
    base
}
