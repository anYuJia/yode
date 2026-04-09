use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::process::Command;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolErrorType, ToolResult};

pub struct GitStatusTool;

#[async_trait]
impl Tool for GitStatusTool {
    fn name(&self) -> &str {
        "git_status"
    }

    fn user_facing_name(&self) -> &str {
        "Git Status"
    }

    fn activity_description(&self, _params: &Value) -> String {
        "Checking git status".to_string()
    }

    fn description(&self) -> &str {
        r#"Shows the working tree status including staged, unstaged, and untracked files.

Use this tool:
- BEFORE git_commit to verify what will be committed
- AFTER git operations to check the result
- To understand the current Git state

Output format:
- Green files (staged): Ready to be committed
- Red files (unstaged): Modified but not staged
- Red "?" (untracked): New files not yet staged

Usage:
- Use `short: true` for compact output (one line per file)
- Without short flag, shows detailed diff with full status messages

Git workflow reminder:
1. git_status - Check current state
2. git_diff - Review changes in detail
3. git_commit - Commit with message"#
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "short": {
                    "type": "boolean",
                    "description": "Use short format output (--short). Default false.",
                    "default": false
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
        let short = params
            .get("short")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let working_dir = ctx
            .working_dir
            .as_deref()
            .unwrap_or_else(|| std::path::Path::new("."));

        let mut cmd = Command::new("git");
        cmd.current_dir(working_dir).arg("status");

        if short {
            cmd.arg("--short");
        }

        let output = cmd
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to run git: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            return Ok(ToolResult::error_typed(
                format!("git status failed: {}", stderr.trim()),
                ToolErrorType::Execution,
                false,
                None,
            ));
        }

        if stdout.is_empty() {
            let metadata = json!({
                "clean": true,
            });
            Ok(ToolResult::success_with_metadata(
                "No changes (clean working tree)".to_string(),
                metadata,
            ))
        } else {
            // Try to extract branch and counts for metadata
            let mut branch = "unknown".to_string();
            let mut staged = 0;
            let mut unstaged = 0;
            let mut untracked = 0;

            // Run git status --porcelain=v2 --branch
            if let Ok(p_out) = Command::new("git")
                .args(["status", "--porcelain=v2", "--branch"])
                .current_dir(working_dir)
                .output()
            {
                let p_stdout = String::from_utf8_lossy(&p_out.stdout);
                for line in p_stdout.lines() {
                    if let Some(stripped) = line.strip_prefix("# branch.head ") {
                        branch = stripped.to_string();
                    } else if line.starts_with('1') || line.starts_with('2') {
                        // Changed tracked file
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() > 1 {
                            let xy = parts[1];
                            // Porcelain v2 XY: X is staged, Y is unstaged
                            // Modified in both: e.g. "MM" or "AM"
                            if xy.starts_with('.') {
                                unstaged += 1;
                            } else if xy.ends_with('.') {
                                staged += 1;
                            } else {
                                staged += 1;
                                unstaged += 1;
                            }
                        }
                    } else if line.starts_with('?') {
                        untracked += 1;
                    }
                }
            }

            let metadata = json!({
                "branch": branch,
                "staged_count": staged,
                "unstaged_count": unstaged,
                "untracked_count": untracked,
                "clean": false,
            });

            Ok(ToolResult::success_with_metadata(
                stdout.to_string(),
                metadata,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::ToolContext;

    fn ctx_with_dir(dir: &std::path::Path) -> ToolContext {
        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.to_path_buf());
        ctx
    }

    fn init_git_repo(dir: &std::path::Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    #[tokio::test]
    async fn test_git_status_clean_repo() {
        let dir = tempfile::tempdir().unwrap();
        init_git_repo(dir.path());
        // Create and commit a file so we have a non-empty repo
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let tool = GitStatusTool;
        let result = tool
            .execute(json!({}), &ctx_with_dir(dir.path()))
            .await
            .unwrap();
        assert!(!result.is_error);
        // Clean repo should show nothing or "clean"
        let content = result.content.to_lowercase();
        assert!(
            content.contains("clean") || content.contains("nothing to commit"),
            "Expected clean status, got: {}",
            result.content
        );
    }

    #[tokio::test]
    async fn test_git_status_with_changes() {
        let dir = tempfile::tempdir().unwrap();
        init_git_repo(dir.path());
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Make a change
        std::fs::write(dir.path().join("b.txt"), "new file").unwrap();

        let tool = GitStatusTool;
        let result = tool
            .execute(json!({}), &ctx_with_dir(dir.path()))
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("b.txt"));
    }

    #[tokio::test]
    async fn test_git_status_short() {
        let dir = tempfile::tempdir().unwrap();
        init_git_repo(dir.path());
        std::fs::write(dir.path().join("x.txt"), "data").unwrap();

        let tool = GitStatusTool;
        let result = tool
            .execute(json!({"short": true}), &ctx_with_dir(dir.path()))
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("x.txt"));
        // Short format uses ?? prefix for untracked
        assert!(result.content.contains("??"));
    }

    #[tokio::test]
    async fn test_git_status_not_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        // Don't init git
        let tool = GitStatusTool;
        let result = tool
            .execute(json!({}), &ctx_with_dir(dir.path()))
            .await
            .unwrap();
        assert!(result.is_error);
    }
}
