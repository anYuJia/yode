use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::process::Command;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolErrorType, ToolResult};

pub struct GitLogTool;

#[async_trait]
impl Tool for GitLogTool {
    fn name(&self) -> &str {
        "git_log"
    }

    fn description(&self) -> &str {
        "Show git commit history. Use to understand recent changes, find commit hashes for git_diff, or check who changed a specific file. Supports filtering by count, path, and author."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "count": {
                    "type": "integer",
                    "description": "Number of commits to show (default 10, max 50)",
                    "default": 10
                },
                "oneline": {
                    "type": "boolean",
                    "description": "Use one-line format. Default false.",
                    "default": false
                },
                "path": {
                    "type": "string",
                    "description": "Optional file path to filter commits"
                },
                "author": {
                    "type": "string",
                    "description": "Optional author name/email filter"
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
        let count = params
            .get("count")
            .and_then(|v| v.as_i64())
            .unwrap_or(10)
            .min(50)
            .max(1) as u32;

        let oneline = params
            .get("oneline")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let path = params.get("path").and_then(|v| v.as_str());
        let author = params.get("author").and_then(|v| v.as_str());

        let working_dir = ctx
            .working_dir
            .as_deref()
            .unwrap_or_else(|| std::path::Path::new("."));

        let mut cmd = Command::new("git");
        cmd.current_dir(working_dir).arg("log");

        cmd.arg(format!("-n{}", count));

        if oneline {
            cmd.arg("--oneline");
        } else {
            cmd.args(["--format=%H %an <%ae> %ad%n  %s%n", "--date=short"]);
        }

        if let Some(a) = author {
            cmd.arg(format!("--author={}", a));
        }

        if let Some(p) = path {
            cmd.args(["--", p]);
        }

        let output = cmd
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to run git: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            return Ok(ToolResult::error_typed(
                format!("git log failed: {}", stderr.trim()),
                ToolErrorType::Execution,
                false,
                None,
            ));
        }

        if stdout.is_empty() {
            Ok(ToolResult::success("No commits found".to_string()))
        } else {
            Ok(ToolResult::success(stdout.to_string()))
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

    fn init_git_repo_with_commits(dir: &std::path::Path, n: usize) {
        Command::new("git").args(["init"]).current_dir(dir).output().unwrap();
        Command::new("git").args(["config", "user.email", "test@test.com"]).current_dir(dir).output().unwrap();
        Command::new("git").args(["config", "user.name", "Tester"]).current_dir(dir).output().unwrap();
        for i in 0..n {
            let fname = format!("file{}.txt", i);
            std::fs::write(dir.join(&fname), format!("content {}", i)).unwrap();
            Command::new("git").args(["add", &fname]).current_dir(dir).output().unwrap();
            Command::new("git").args(["commit", "-m", &format!("commit {}", i)]).current_dir(dir).output().unwrap();
        }
    }

    #[tokio::test]
    async fn test_git_log_default() {
        let dir = tempfile::tempdir().unwrap();
        init_git_repo_with_commits(dir.path(), 3);

        let tool = GitLogTool;
        let result = tool.execute(json!({}), &ctx_with_dir(dir.path())).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("commit 2"));
        assert!(result.content.contains("commit 0"));
    }

    #[tokio::test]
    async fn test_git_log_count_limit() {
        let dir = tempfile::tempdir().unwrap();
        init_git_repo_with_commits(dir.path(), 5);

        let tool = GitLogTool;
        let result = tool.execute(json!({"count": 2, "oneline": true}), &ctx_with_dir(dir.path())).await.unwrap();
        assert!(!result.is_error);
        // Should only show 2 commits (most recent)
        let lines: Vec<&str> = result.content.trim().lines().collect();
        assert_eq!(lines.len(), 2, "Expected 2 lines, got: {:?}", lines);
        assert!(result.content.contains("commit 4"));
    }

    #[tokio::test]
    async fn test_git_log_path_filter() {
        let dir = tempfile::tempdir().unwrap();
        init_git_repo_with_commits(dir.path(), 3);

        let tool = GitLogTool;
        let result = tool.execute(json!({"path": "file1.txt", "oneline": true}), &ctx_with_dir(dir.path())).await.unwrap();
        assert!(!result.is_error);
        let lines: Vec<&str> = result.content.trim().lines().collect();
        assert_eq!(lines.len(), 1, "Expected 1 commit for file1.txt, got: {:?}", lines);
    }

    #[tokio::test]
    async fn test_git_log_author_filter() {
        let dir = tempfile::tempdir().unwrap();
        init_git_repo_with_commits(dir.path(), 2);

        let tool = GitLogTool;
        let result = tool.execute(json!({"author": "Tester", "oneline": true}), &ctx_with_dir(dir.path())).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("commit"));

        // Non-matching author
        let result = tool.execute(json!({"author": "nobody", "oneline": true}), &ctx_with_dir(dir.path())).await.unwrap();
        assert!(!result.is_error);
        assert_eq!(result.content, "No commits found");
    }

    #[tokio::test]
    async fn test_git_log_count_clamped() {
        let dir = tempfile::tempdir().unwrap();
        init_git_repo_with_commits(dir.path(), 2);

        let tool = GitLogTool;
        // count=100 should be clamped to 50, but we only have 2 commits
        let result = tool.execute(json!({"count": 100, "oneline": true}), &ctx_with_dir(dir.path())).await.unwrap();
        assert!(!result.is_error);
        let lines: Vec<&str> = result.content.trim().lines().collect();
        assert_eq!(lines.len(), 2);

        // count=0 should be clamped to 1
        let result = tool.execute(json!({"count": 0, "oneline": true}), &ctx_with_dir(dir.path())).await.unwrap();
        assert!(!result.is_error);
        let lines: Vec<&str> = result.content.trim().lines().collect();
        assert_eq!(lines.len(), 1);
    }
}
