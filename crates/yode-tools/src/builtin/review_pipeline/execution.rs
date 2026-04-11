use anyhow::Result;
use serde_json::{json, Value};

use crate::builtin::git_commit::GitCommitTool;
use crate::builtin::review_changes::ReviewChangesTool;
use crate::builtin::review_common::{
    persist_review_artifact, persist_review_status, review_findings_count,
    review_metadata_payload, review_output_has_findings,
};
use crate::builtin::test_runner::TestRunnerTool;
use crate::builtin::verification_agent::VerificationAgentTool;
use crate::tool::{Tool, ToolContext, ToolErrorType, ToolResult};

pub(super) async fn execute_review_pipeline(
    params: Value,
    ctx: &ToolContext,
) -> Result<ToolResult> {
    let focus = params
        .get("focus")
        .and_then(|value| value.as_str())
        .unwrap_or("current workspace changes");
    let review_instructions = params
        .get("review_instructions")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let verification_goal = params
        .get("verification_goal")
        .and_then(|value| value.as_str())
        .unwrap_or("verify the current implementation is correct");
    let verification_instructions = params
        .get("verification_instructions")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let test_command = params.get("test_command").and_then(|value| value.as_str());
    let commit_message = params
        .get("commit_message")
        .and_then(|value| value.as_str());
    let allow_findings_commit = params
        .get("allow_findings_commit")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let review_result = ReviewChangesTool
        .execute(
            json!({
                "focus": focus,
                "instructions": review_instructions,
                "run_in_background": false,
            }),
            ctx,
        )
        .await?;
    let review_output = review_result.content.clone();
    let review_failed = review_output_has_findings(&review_output);
    let review_findings = review_findings_count(&review_output);

    let verification_result = VerificationAgentTool
        .execute(
            json!({
                "goal": verification_goal,
                "focus": focus,
                "instructions": verification_instructions,
                "run_in_background": false,
            }),
            ctx,
        )
        .await?;
    let verification_output = verification_result.content.clone();
    let verification_failed = review_output_has_findings(&verification_output);
    let verification_findings = review_findings_count(&verification_output);

    let mut test_result = None;
    if let Some(command) = test_command {
        test_result = Some(
            TestRunnerTool
                .execute(json!({ "command": command }), ctx)
                .await
                .unwrap_or_else(|err| ToolResult::error(format!("Test runner failed: {}", err))),
        );
    }

    let should_stop_for_findings = (review_failed || verification_failed) && !allow_findings_commit;
    let mut commit_result = None;
    if let Some(message) = commit_message {
        if !should_stop_for_findings {
            commit_result = Some(
                GitCommitTool
                    .execute(
                        json!({
                            "message": message,
                            "files": params.get("files").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
                            "all": params.get("all").cloned().unwrap_or_else(|| Value::Bool(false)),
                        }),
                        ctx,
                    )
                    .await?,
            );
        }
    }

    let summary = format!(
        "Review:\n{}\n\nVerification:\n{}\n\nTests:\n{}\n\nCommit:\n{}",
        review_output,
        verification_output,
        test_result
            .as_ref()
            .map(|result| result.content.clone())
            .unwrap_or_else(|| "not run".to_string()),
        commit_result
            .as_ref()
            .map(|result| result.content.clone())
            .unwrap_or_else(|| {
                if commit_message.is_some() && should_stop_for_findings {
                    "skipped due to findings".to_string()
                } else {
                    "not requested".to_string()
                }
            })
    );

    let pipeline_artifact = ctx
        .working_dir
        .as_deref()
        .and_then(|dir| persist_review_artifact(dir, "review-pipeline", focus, &summary).ok())
        .map(|path| path.display().to_string());
    if let (Some(dir), Some(path)) = (
        ctx.working_dir.as_deref(),
        pipeline_artifact.as_deref().map(std::path::Path::new),
    ) {
        let _ = persist_review_status(dir, "review-pipeline", focus, &summary, Some(path));
    }

    if should_stop_for_findings {
        return Ok(ToolResult {
            content: format!(
                "Review pipeline detected findings. Commit skipped.\n\n{}",
                summary
            ),
            is_error: true,
            error_type: Some(ToolErrorType::Validation),
            recoverable: true,
            suggestion: Some(
                "Address review or verification findings first, or set allow_findings_commit=true to override."
                    .to_string(),
            ),
            metadata: Some(merge_review_metadata(
                review_metadata_payload("review-pipeline", focus, &summary, pipeline_artifact.as_deref()),
                json!({
                "focus": focus,
                "review_output": review_output,
                "review_findings_count": review_findings,
                "verification_output": verification_output,
                "verification_findings_count": verification_findings,
                "total_findings_count": review_findings + verification_findings,
                "pipeline_artifact_path": pipeline_artifact,
                "commit_skipped": true,
                }),
            )),
        });
    }

    Ok(ToolResult::success_with_metadata(
        format!("Review pipeline complete.\n\n{}", summary),
        merge_review_metadata(
            review_metadata_payload("review-pipeline", focus, &summary, pipeline_artifact.as_deref()),
            json!({
            "focus": focus,
            "review_output": review_output,
            "review_findings_count": review_findings,
            "verification_output": verification_output,
            "verification_findings_count": verification_findings,
            "total_findings_count": review_findings + verification_findings,
            "pipeline_artifact_path": pipeline_artifact,
            "test_ran": test_result.is_some(),
            "committed": commit_result.is_some(),
            }),
        ),
    ))
}

fn merge_review_metadata(mut base: Value, extra: Value) -> Value {
    if let (Some(base_object), Some(extra_object)) = (base.as_object_mut(), extra.as_object()) {
        for (key, value) in extra_object {
            base_object.insert(key.clone(), value.clone());
        }
    }
    base
}
