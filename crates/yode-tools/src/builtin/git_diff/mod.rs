use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::process::Command;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolErrorType, ToolResult};

pub struct GitDiffTool;

#[async_trait]
impl Tool for GitDiffTool {
    fn name(&self) -> &str {
        "git_diff"
    }

    fn user_facing_name(&self) -> &str {
        "Git Diff"
    }

    fn activity_description(&self, params: &Value) -> String {
        let target = params.get("target").and_then(|v| v.as_str()).unwrap_or("unstaged");
        format!("Showing git diff ({})", target)
    }

    fn description(&self) -> &str {
        r#"Shows git diff for staged changes, unstaged changes, or a specific commit.

Use this tool:
- To review changes before committing
- To understand what a previous commit changed
- To check specific file modifications

Parameters:
- `target`: "staged" (cached changes), "unstaged" (working tree changes), or "commit" (compare to specific commit)
- `commit`: Required when target is "commit". Use hashes like "HEAD", "HEAD~1", "abc1234"
- `path`: Optional filter to limit diff to specific files

Common patterns:
- Unstaged changes: {target: "unstaged"}
- Staged changes: {target: "staged"}
- Compare to last commit: {target: "commit", commit: "HEAD"}
- Specific file: {target: "unstaged", path: "src/lib.rs"}

Output: Unified diff format showing added (+) and removed (-) lines."#
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "What to diff: \"staged\" (cached changes), \"unstaged\" (working tree), or \"commit\" (specific commit)",
                    "default": "unstaged"
                },
                "commit": {
                    "type": "string",
                    "description": "Commit hash or ref (required when target is \"commit\")"
                },
                "path": {
                    "type": "string",
                    "description": "Optional path filter to limit diff to specific files"
                }
            }
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: true,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let target = params
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("unstaged");
        let commit = params.get("commit").and_then(|v| v.as_str());
        let path = params.get("path").and_then(|v| v.as_str());

        let working_dir = ctx
            .working_dir
            .as_deref()
            .unwrap_or_else(|| std::path::Path::new("."));

        let mut cmd = Command::new("git");
        cmd.current_dir(working_dir);

        match target {
            "staged" => {
                cmd.args(["diff", "--staged"]);
            }
            "unstaged" => {
                cmd.arg("diff");
            }
            "commit" => {
                let hash = match commit {
                    Some(c) => c,
                    None => {
                        return Ok(ToolResult::error_typed(
                            "Parameter 'commit' is required when target is \"commit\"".to_string(),
                            ToolErrorType::Validation,
                            true,
                            Some("Provide a commit hash or ref, e.g. \"HEAD~1\" or \"abc1234\"".to_string()),
                        ));
                    }
                };
                cmd.args(["diff", hash]);
            }
            other => {
                return Ok(ToolResult::error_typed(
                    format!("Invalid target: \"{}\". Must be \"staged\", \"unstaged\", or \"commit\"", other),
                    ToolErrorType::Validation,
                    true,
                    Some("Use target: \"staged\", \"unstaged\", or \"commit\"".to_string()),
                ));
            }
        }

        if let Some(p) = path {
            cmd.args(["--", p]);
        }

        let output = cmd.output().map_err(|e| anyhow::anyhow!("Failed to run git: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            return Ok(ToolResult::error_typed(
                format!("git diff failed: {}", stderr.trim()),
                ToolErrorType::Execution,
                false,
                None,
            ));
        }

        if stdout.is_empty() {
            let metadata = json!({
                "target": target,
                "path": path,
                "has_changes": false,
            });
            Ok(ToolResult::success_with_metadata(format!("No differences found (target: {})", target), metadata))
        } else {
            // Count files in diff (lines starting with "diff --git")
            let file_count = stdout.lines().filter(|l| l.starts_with("diff --git")).count();
            let metadata = json!({
                "target": target,
                "path": path,
                "has_changes": true,
                "files_changed": file_count,
            });
            Ok(ToolResult::success_with_metadata(stdout.to_string(), metadata))
        }
    }
}
