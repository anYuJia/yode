use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct EnterWorktreeTool;
pub struct ExitWorktreeTool;

#[async_trait]
impl Tool for EnterWorktreeTool {
    fn name(&self) -> &str {
        "enter_worktree"
    }

    fn user_facing_name(&self) -> &str {
        "Enter Worktree"
    }

    fn activity_description(&self, params: &Value) -> String {
        let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("new");
        format!("Creating worktree: {}", name)
    }

    fn description(&self) -> &str {
        "Create an isolated git worktree and switch the session into it. \
         Use this when you need to work on a feature in isolation from the current workspace."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Optional name for the worktree. A random name is generated if not provided."
                }
            }
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
        let working_dir = ctx
            .working_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Working directory not set"))?;

        let worktree_state = ctx
            .worktree_state
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Worktree state not available"))?;

        // Check if already in a worktree
        {
            let state = worktree_state.lock().await;
            if state.current_worktree.is_some() {
                return Ok(ToolResult::error(
                    "Already in a worktree. Exit the current one first.".to_string(),
                ));
            }
        }

        // Check if in a git repo
        if !working_dir.join(".git").exists() {
            return Ok(ToolResult::error(
                "Not in a git repository. Worktrees require git.".to_string(),
            ));
        }

        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("wt-{}", &uuid::Uuid::new_v4().to_string()[..8]));

        let branch_name = format!("yode-{}", name);
        let worktree_dir = working_dir.join(".yode").join("worktrees").join(&name);

        // Create worktree directory
        std::fs::create_dir_all(worktree_dir.parent().unwrap())?;

        // git worktree add
        let output = std::process::Command::new("git")
            .args([
                "worktree",
                "add",
                &worktree_dir.display().to_string(),
                "-b",
                &branch_name,
                "HEAD",
            ])
            .current_dir(working_dir)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to run git worktree add: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Ok(ToolResult::error(format!(
                "git worktree add failed: {}",
                stderr
            )));
        }

        // Update state
        {
            let mut state = worktree_state.lock().await;
            state.original_dir = Some(working_dir.clone());
            state.current_worktree = Some(worktree_dir.clone());
            state.branch_name = Some(branch_name.clone());
        }

        let metadata = serde_json::json!({
            "path": worktree_dir.display().to_string(),
            "branch": branch_name,
        });

        Ok(ToolResult::success_with_metadata(
            format!("Created worktree at {} on branch '{}'. Session is now in the worktree.", worktree_dir.display(), branch_name),
            metadata
        ))
    }
}

#[async_trait]
impl Tool for ExitWorktreeTool {
    fn name(&self) -> &str {
        "exit_worktree"
    }

    fn user_facing_name(&self) -> &str {
        "Exit Worktree"
    }

    fn activity_description(&self, params: &Value) -> String {
        let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("keep");
        format!("Exiting worktree (action: {})", action)
    }

    fn description(&self) -> &str {
        "Exit the current worktree session. Use action 'keep' to preserve the worktree, \
         or 'remove' to delete it and its branch."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["keep", "remove"],
                    "description": "'keep' preserves the worktree, 'remove' deletes it and the branch"
                },
                "discard_changes": {
                    "type": "boolean",
                    "default": false,
                    "description": "If true, force remove even with uncommitted changes"
                }
            },
            "required": ["action"]
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
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("keep");
        let discard_changes = params
            .get("discard_changes")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let worktree_state = ctx
            .worktree_state
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Worktree state not available"))?;

        let (original_dir, worktree_dir, branch_name) = {
            let state = worktree_state.lock().await;
            match (&state.original_dir, &state.current_worktree, &state.branch_name) {
                (Some(orig), Some(wt), Some(branch)) => {
                    (orig.clone(), wt.clone(), branch.clone())
                }
                _ => {
                    return Ok(ToolResult::error(
                        "No active worktree session.".to_string(),
                    ));
                }
            }
        };

        if action == "remove" {
            // Check for uncommitted changes
            if !discard_changes {
                let status = std::process::Command::new("git")
                    .args(["status", "--porcelain"])
                    .current_dir(&worktree_dir)
                    .output();

                if let Ok(output) = status {
                    let changes = String::from_utf8_lossy(&output.stdout);
                    if !changes.trim().is_empty() {
                        return Ok(ToolResult::error(format!(
                            "Worktree has uncommitted changes:\n{}\n\
                             Set discard_changes=true to force remove.",
                            changes
                        )));
                    }
                }
            }

            // Remove worktree
            let output = std::process::Command::new("git")
                .args(["worktree", "remove", &worktree_dir.display().to_string(), "--force"])
                .current_dir(&original_dir)
                .output()
                .map_err(|e| anyhow::anyhow!("Failed to remove worktree: {}", e))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Ok(ToolResult::error(format!(
                    "git worktree remove failed: {}",
                    stderr
                )));
            }

            // Delete branch
            let _ = std::process::Command::new("git")
                .args(["branch", "-D", &branch_name])
                .current_dir(&original_dir)
                .output();
        }

        // Clear state
        {
            let mut state = worktree_state.lock().await;
            state.original_dir = None;
            state.current_worktree = None;
            state.branch_name = None;
        }

        let msg = if action == "remove" {
            format!("Worktree removed. Session restored to {}", original_dir.display())
        } else {
            format!(
                "Exited worktree (kept at {}). Session restored to {}",
                worktree_dir.display(),
                original_dir.display()
            )
        };

        let metadata = serde_json::json!({
            "action": action,
            "original_dir": original_dir.display().to_string(),
        });

        Ok(ToolResult::success_with_metadata(msg, metadata))
    }
}
