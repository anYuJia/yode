use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::process::Command;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolErrorType, ToolResult};

pub struct GitCommitTool;

#[async_trait]
impl Tool for GitCommitTool {
    fn name(&self) -> &str {
        "git_commit"
    }

    fn description(&self) -> &str {
        r#"Creates a git commit with staged changes.

Before using this tool:
1. Use git_status to check the current state
2. Use git_diff to review changes
3. Ensure you understand what will be committed

Usage:
- Provide a clear, descriptive commit message that explains the "why" not just the "what"
- Use the `files` parameter to stage specific files before committing
- Use `all: true` to stage all tracked modified files (like git commit -a)
- Untracked files must be explicitly staged using the `files` parameter

Commit message guidelines:
- Start with a verb in present tense (e.g., "Add", "Fix", "Update", "Refactor")
- Keep the first line under 50 characters
- Add a body if more detail is needed, separated by a blank line
- Reference issue numbers when applicable (e.g., "Fix login bug (#123)")

Git safety:
- NEVER skip hooks (--no-verify, --no-gpg-sign) unless explicitly requested
- NEVER amend commits (--amend) unless specifically asked
- Always verify the commit was successful using git_status

After committing, the tool returns the commit output. Use git_status to verify the commit state."#
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Commit message"
                },
                "files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Files to stage before committing (git add)"
                },
                "all": {
                    "type": "boolean",
                    "description": "Stage all tracked modified files (-a). Default false.",
                    "default": false
                }
            },
            "required": ["message"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: true,
            supports_auto_execution: false,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let message = match params.get("message").and_then(|v| v.as_str()) {
            Some(m) if !m.is_empty() => m,
            _ => {
                return Ok(ToolResult::error_typed(
                    "Parameter 'message' is required and must not be empty".to_string(),
                    ToolErrorType::Validation,
                    true,
                    Some("Provide a commit message".to_string()),
                ));
            }
        };

        let files: Vec<&str> = params
            .get("files")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        let all = params
            .get("all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let working_dir = ctx
            .working_dir
            .as_deref()
            .unwrap_or_else(|| std::path::Path::new("."));

        // Stage files if specified
        if !files.is_empty() {
            let mut add_cmd = Command::new("git");
            add_cmd.current_dir(working_dir).arg("add");
            for f in &files {
                add_cmd.arg(f);
            }
            let add_output = add_cmd
                .output()
                .map_err(|e| anyhow::anyhow!("Failed to run git add: {}", e))?;

            if !add_output.status.success() {
                let stderr = String::from_utf8_lossy(&add_output.stderr);
                return Ok(ToolResult::error_typed(
                    format!("git add failed: {}", stderr.trim()),
                    ToolErrorType::Execution,
                    false,
                    None,
                ));
            }
        }

        // Commit
        let mut cmd = Command::new("git");
        cmd.current_dir(working_dir).arg("commit");

        if all {
            cmd.arg("-a");
        }

        cmd.args(["-m", message]);

        let output = cmd
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to run git commit: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            return Ok(ToolResult::error_typed(
                format!("git commit failed: {}", stderr.trim()),
                ToolErrorType::Execution,
                false,
                Some("Check that there are staged changes to commit".to_string()),
            ));
        }

        Ok(ToolResult::success(stdout.to_string()))
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

    fn init_repo(dir: &std::path::Path) {
        Command::new("git").args(["init"]).current_dir(dir).output().unwrap();
        Command::new("git").args(["config", "user.email", "test@test.com"]).current_dir(dir).output().unwrap();
        Command::new("git").args(["config", "user.name", "Test"]).current_dir(dir).output().unwrap();
    }

    #[tokio::test]
    async fn test_commit_with_files() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();

        let tool = GitCommitTool;
        let result = tool.execute(json!({
            "message": "add a.txt",
            "files": ["a.txt"]
        }), &ctx_with_dir(dir.path())).await.unwrap();
        assert!(!result.is_error, "commit failed: {}", result.content);

        // Verify commit was made
        let log = Command::new("git").args(["log", "--oneline"]).current_dir(dir.path()).output().unwrap();
        let log_str = String::from_utf8_lossy(&log.stdout);
        assert!(log_str.contains("add a.txt"));
    }

    #[tokio::test]
    async fn test_commit_with_all_flag() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        // Create and commit initial file
        std::fs::write(dir.path().join("a.txt"), "v1").unwrap();
        Command::new("git").args(["add", "."]).current_dir(dir.path()).output().unwrap();
        Command::new("git").args(["commit", "-m", "init"]).current_dir(dir.path()).output().unwrap();

        // Modify tracked file
        std::fs::write(dir.path().join("a.txt"), "v2").unwrap();

        let tool = GitCommitTool;
        let result = tool.execute(json!({
            "message": "update a.txt",
            "all": true
        }), &ctx_with_dir(dir.path())).await.unwrap();
        assert!(!result.is_error, "commit -a failed: {}", result.content);
    }

    #[tokio::test]
    async fn test_commit_empty_message_fails() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());

        let tool = GitCommitTool;
        let result = tool.execute(json!({
            "message": ""
        }), &ctx_with_dir(dir.path())).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("message"));
    }

    #[tokio::test]
    async fn test_commit_nothing_staged_fails() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        // Need an initial commit for git to exist properly
        std::fs::write(dir.path().join("a.txt"), "v1").unwrap();
        Command::new("git").args(["add", "."]).current_dir(dir.path()).output().unwrap();
        Command::new("git").args(["commit", "-m", "init"]).current_dir(dir.path()).output().unwrap();

        let tool = GitCommitTool;
        let result = tool.execute(json!({
            "message": "nothing to commit"
        }), &ctx_with_dir(dir.path())).await.unwrap();
        assert!(result.is_error, "Should fail with nothing staged");
    }
}
