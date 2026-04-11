mod background;
mod output;
mod watchdog;

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::process::Command;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolErrorType, ToolProgress, ToolResult};

const DEFAULT_TIMEOUT_SECS: u64 = 120;
const MAX_TIMEOUT_SECS: u64 = 600;
const STALL_CHECK_INTERVAL_MS: u64 = 5_000;
const STALL_THRESHOLD_MS: u64 = 45_000;
const STALL_TAIL_BYTES: usize = 1024;

const INTERACTIVE_PROMPT_PATTERNS: &[&str] = &[
    "password:",
    "Password:",
    "passphrase",
    "[y/n]",
    "[Y/n]",
    "[yes/no]",
    "Are you sure",
    "are you sure",
    "Continue?",
    "continue?",
    "Press any key",
    "press any key",
    "Enter ",
    "enter ",
    "Username:",
    "username:",
    "(yes/no)",
    "(Y/N)",
    "Do you want to",
    "do you want to",
    "> ",
    "$ ",
    "# ",
];

pub struct BashTool;

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn user_facing_name(&self) -> &str {
        "Bash"
    }

    fn activity_description(&self, params: &Value) -> String {
        let command = params.get("command").and_then(|v| v.as_str()).unwrap_or("");
        format!("Running command: {}", command)
    }

    fn description(&self) -> &str {
        r#"Executes a given bash command and returns its output.

The working directory persists between commands, but shell state does not. The shell environment is initialized from the user's profile (bash or zsh).

IMPORTANT: Avoid using this tool to run `find`, `grep`, `cat`, `head`, `tail`, `sed`, `awk`, or `echo` commands, unless explicitly instructed or after you have verified that a dedicated tool cannot accomplish your task. Instead, use the appropriate dedicated tool as this will provide a much better experience for the user:
- File search: Use `glob` (NOT find or ls)
- Content search: Use `grep` (NOT grep or rg)
- Read files: Use `read_file` (NOT cat/head/tail)
- Edit files: Use `edit_file` (NOT sed/awk)
- Write files: Use `write_file` (NOT echo >/cat <<EOF)
- Communication: Output text directly (NOT echo/printf)

While the bash tool can do similar things, it's better to use the built-in tools as they provide a better user experience and make it easier to review tool calls and give permission.

# Instructions
- If your command will create new directories or files, first use this tool to run `ls` to verify the parent directory exists and is the correct location.
- Always quote file paths that contain spaces with double quotes in your command (e.g., cd "path with spaces/file.txt")
- Try to maintain your current working directory throughout the session by using absolute paths and avoiding usage of `cd`. You may use `cd` if the User explicitly requests it.
- You may specify an optional timeout in milliseconds (up to 600000ms / 10 minutes). By default, your command will timeout after 120000ms (2 minutes).
- You can use the `run_in_background` parameter to run the command in the background. Only use this if you don't need the result immediately and are OK being notified when the command completes later. You do not need to use '&' at the end of the command when using this parameter.
- When issuing multiple commands:
  - If the commands are independent and can run in parallel, make multiple bash tool calls in a single message. Example: if you need to run "git status" and "git diff", send a single message with two bash tool calls in parallel.
  - If the commands depend on each other and must run sequentially, use a single bash call with '&&' to chain them together.
  - Use ';' only when you need to run commands sequentially but don't care if earlier commands fail.
  - DO NOT use newlines to separate commands (newlines are ok in quoted strings).

# For git commands:
- Prefer to create a new commit rather than amending an existing commit.
- Before running destructive operations (e.g., git reset --hard, git push --force, git checkout --), consider whether there is a safer alternative that achieves the same goal. Only use destructive operations when they are truly the best approach.
- Never skip hooks (--no-verify) or bypass signing (--no-gpg-sign, -c commit.gpgsign=false) unless the user has explicitly asked for it. If a hook fails, investigate and fix the underlying issue.
- In order to ensure good formatting, ALWAYS pass the commit message via a HEREDOC, e.g.:
  git commit -m "$(cat <<'EOF'
  Commit message here.
  EOF
  )"

# Avoid unnecessary `sleep` commands:
- Do not sleep between commands that can run immediately — just run them.
- If your command is long running and you would like to be notified when it finishes — use `run_in_background`. No sleep needed.
- Do not retry failing commands in a sleep loop — diagnose the root cause.
- If waiting for a background task you started with `run_in_background`, you will be notified when it completes — do not poll.
- If you must poll an external process, use a check command (e.g. `gh run view`) rather than sleeping first.
- If you must sleep, keep the duration short (1-5 seconds) to avoid blocking the user."#
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command to execute"
                },
                "description": {
                    "type": "string",
                    "description": "A short (3-5 word) description of the task being performed by the command"
                },
                "run_in_background": {
                    "type": "boolean",
                    "default": false,
                    "description": "Whether to run the command in the background."
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": format!("Optional timeout in milliseconds (max {}ms). Default: {}ms.", MAX_TIMEOUT_SECS * 1000, DEFAULT_TIMEOUT_SECS * 1000)
                },
                "dangerously_disable_sandbox": {
                    "type": "boolean",
                    "default": false,
                    "description": "Whether to disable the command sandbox. Only use this if the command fails due to sandbox restrictions."
                }
            },
            "required": ["command"]
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
        let command = params
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: command"))?;

        let working_dir = ctx.working_dir.as_deref().unwrap_or_else(|| Path::new("."));

        let timeout_ms = params
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .or_else(|| params.get("timeout").and_then(|v| v.as_u64()));

        let timeout_secs = match timeout_ms {
            Some(t) if t >= 1000 => (t / 1000).min(MAX_TIMEOUT_SECS),
            Some(t) => t.min(MAX_TIMEOUT_SECS),
            None => DEFAULT_TIMEOUT_SECS,
        };

        let run_in_background = params
            .get("run_in_background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        tracing::debug!(
            command = %command,
            timeout_secs = timeout_secs,
            background = run_in_background,
            "Executing bash command"
        );

        if run_in_background {
            return self.execute_background(command, working_dir, ctx).await;
        }

        let timeout_duration = Duration::from_secs(timeout_secs);

        let mut child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(working_dir)
            .spawn()?;

        let stall_check = self
            .run_with_stall_watchdog(&mut child, timeout_duration, ctx.progress_tx.clone())
            .await;

        match stall_check {
            watchdog::StallResult::Completed(output) => {
                self.format_output(command, working_dir, output)
            }
            watchdog::StallResult::Stalled(partial_output) => {
                let _ = child.kill().await;
                Ok(ToolResult::error_typed(
                    format!(
                        "Command appears to be stalled (waiting for interactive input).\n\
                         Last output:\n{}\n\n\
                         The command was killed. If it requires interactive input, \
                         try using 'yes |' prefix or pass flags like '-y' or '--yes'.",
                        partial_output
                    ),
                    ToolErrorType::Timeout,
                    true,
                    Some("Add non-interactive flags or pipe input to avoid stalling.".to_string()),
                ))
            }
            watchdog::StallResult::Timeout => {
                let _ = child.kill().await;
                Ok(ToolResult::error_typed(
                    format!("Command timed out after {} seconds", timeout_secs),
                    ToolErrorType::Timeout,
                    true,
                    Some("Increase timeout or reduce scope.".to_string()),
                ))
            }
            watchdog::StallResult::Error(e) => {
                let _ = child.kill().await;
                Ok(ToolResult::error(format!(
                    "Failed to execute command: {}",
                    e
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests;
