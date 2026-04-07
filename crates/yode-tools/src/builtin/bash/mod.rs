use std::process::Stdio;
use std::time::Duration;
use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult, ToolErrorType, ToolProgress};

const DEFAULT_TIMEOUT_SECS: u64 = 120;
const MAX_TIMEOUT_SECS: u64 = 600;

/// Stall watchdog constants
const STALL_CHECK_INTERVAL_MS: u64 = 5_000;
const STALL_THRESHOLD_MS: u64 = 45_000;
const STALL_TAIL_BYTES: usize = 1024;

/// Patterns that suggest an interactive prompt is blocking.
const INTERACTIVE_PROMPT_PATTERNS: &[&str] = &[
    "password:", "Password:", "passphrase",
    "[y/n]", "[Y/n]", "[yes/no]",
    "Are you sure", "are you sure",
    "Continue?", "continue?",
    "Press any key", "press any key",
    "Enter ", "enter ",
    "Username:", "username:",
    "(yes/no)", "(Y/N)",
    "Do you want to", "do you want to",
    "> ", "$ ", "# ",
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
        let command = params
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");
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
- When issuing multiple commands:
  - If the commands are independent and can run in parallel, make multiple bash tool calls in a single message. Example: if you need to run "git status" and "git diff", send a single message with two bash tool calls in parallel.
  - If the commands depend on each other and must run sequentially, use a single bash call with '&&' to chain them together.
  - Use ';' only when you need to run commands sequentially but don't care if earlier commands fail.
  - DO NOT use newlines to separate commands (newlines are ok in quoted strings).

# For git commands:
- Prefer to create a new commit rather than amending an existing commit.
- Before running destructive operations (e.g., git reset --hard, git push --force, git checkout --), consider whether there is a safer alternative that achieves the same goal. Only use destructive operations when they are truly the best approach.
- Never skip hooks (--no-verify) or bypass signing (--no-gpg-sign, -c commit.gpgsign=false) unless the user has explicitly asked for it. If a hook fails, investigate and fix the underlying issue.

# Avoid unnecessary `sleep` commands:
- Do not sleep between commands that can run immediately — just run them.
- If your command is long running and you would like to be notified when it finishes — use `run_in_background`. No sleep needed.
- Do not retry failing commands in a sleep loop — diagnose the root cause.
- If waiting for a background task you started with `run_in_background`, you will be notified when it completes — do not poll.
- If you must poll an external process, use a check command (e.g. `gh run view`) rather than sleeping first.
- If you must sleep, keep the duration short (1-5 seconds) to avoid blocking the user.

# run_in_background parameter
Set to true to run this command in the background. Only use this if you don't need the result immediately and are OK being notified when the command completes later. You do not need to check the output right away - you'll be notified when it finishes. You do not need to use '&' at the end of the command when using this parameter."#
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute"
                },
                "timeout": {
                    "type": "integer",
                    "description": format!("Timeout in milliseconds (max {}ms). Default: {}ms", MAX_TIMEOUT_SECS * 1000, DEFAULT_TIMEOUT_SECS * 1000)
                },
                "description": {
                    "type": "string",
                    "description": "Clear, concise description of what this command does"
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Run command in background (default: false)"
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

        let working_dir = ctx
            .working_dir
            .as_deref()
            .unwrap_or_else(|| Path::new("."));

        // Parse timeout: accept both seconds (< 1000) and milliseconds
        let timeout_raw = params.get("timeout").and_then(|v| v.as_u64());
        let timeout_secs = match timeout_raw {
            Some(t) if t > 1000 => (t / 1000).min(MAX_TIMEOUT_SECS),
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
            return self.execute_background(command).await;
        }

        let timeout_duration = Duration::from_secs(timeout_secs);

        let mut child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(working_dir)
            .spawn()?;

        // Stall watchdog: periodically check if the command is stalled
        let stall_check = self.run_with_stall_watchdog(&mut child, timeout_duration, ctx.progress_tx.clone()).await;

        match stall_check {
            StallResult::Completed(output) => self.format_output(command, working_dir, output),
            StallResult::Stalled(partial_output) => {
                // Kill the stalled process
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
            StallResult::Timeout => {
                let _ = child.kill().await;
                Ok(ToolResult::error_typed(
                    format!("Command timed out after {} seconds", timeout_secs),
                    ToolErrorType::Timeout,
                    true,
                    Some("Increase timeout or reduce scope.".to_string()),
                ))
            }
            StallResult::Error(e) => {
                let _ = child.kill().await;
                Ok(ToolResult::error(format!("Failed to execute command: {}", e)))
            }
        }
    }
}

enum StallResult {
    Completed(std::process::Output),
    Stalled(String),
    Timeout,
    Error(String),
}

impl BashTool {
    async fn run_with_stall_watchdog(
        &self,
        child: &mut tokio::process::Child,
        timeout: Duration,
        progress_tx: Option<mpsc::UnboundedSender<ToolProgress>>,
    ) -> StallResult {
        let start = std::time::Instant::now();
        let mut last_output_time = std::time::Instant::now();
        let mut accumulated_stdout = Vec::new();

        // Take stdout for monitoring
        let mut stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                // Fall back to simple wait
                return match tokio::time::timeout(timeout, child.wait()).await {
                    Ok(Ok(status)) => {
                        let mut stderr_buf = Vec::new();
                        if let Some(mut stderr) = child.stderr.take() {
                            let _ = stderr.read_to_end(&mut stderr_buf).await;
                        }
                        StallResult::Completed(std::process::Output {
                            status,
                            stdout: Vec::new(),
                            stderr: stderr_buf,
                        })
                    }
                    Ok(Err(e)) => StallResult::Error(e.to_string()),
                    Err(_) => StallResult::Timeout,
                };
            }
        };

        let mut buf = vec![0u8; 4096];

        loop {
            let elapsed = start.elapsed();
            if elapsed >= timeout {
                return StallResult::Timeout;
            }

            let remaining = timeout - elapsed;
            let check_interval = Duration::from_millis(STALL_CHECK_INTERVAL_MS);
            let wait_time = remaining.min(check_interval);

            tokio::select! {
                n = stdout.read(&mut buf) => {
                    match n {
                        Ok(0) => {
                            // EOF - process finished writing
                            // Wait for process to exit
                            let remaining = timeout.saturating_sub(start.elapsed());
                            match tokio::time::timeout(remaining, child.wait()).await {
                                Ok(Ok(status)) => {
                                    // Collect stderr
                                    let mut stderr_buf = Vec::new();
                                    if let Some(mut stderr) = child.stderr.take() {
                                        let _ = stderr.read_to_end(&mut stderr_buf).await;
                                    }
                                    return StallResult::Completed(std::process::Output {
                                        status,
                                        stdout: accumulated_stdout,
                                        stderr: stderr_buf,
                                    });
                                }
                                Ok(Err(e)) => return StallResult::Error(e.to_string()),
                                Err(_) => return StallResult::Timeout,
                            }
                        }
                        Ok(n) => {
                            let chunk = &buf[..n];
                            if let Some(ref tx) = progress_tx {
                                let message = String::from_utf8_lossy(chunk).to_string();
                                let _ = tx.send(ToolProgress {
                                    message,
                                    percent: None,
                                });
                            }
                            accumulated_stdout.extend_from_slice(chunk);
                            last_output_time = std::time::Instant::now();
                        }
                        Err(e) => return StallResult::Error(e.to_string()),
                    }
                }
                _ = tokio::time::sleep(wait_time) => {
                    // Check for stall
                    let stall_duration = last_output_time.elapsed();
                    if stall_duration >= Duration::from_millis(STALL_THRESHOLD_MS) {
                        // Check if the last output looks like an interactive prompt
                        let tail_start = accumulated_stdout.len().saturating_sub(STALL_TAIL_BYTES);
                        let tail = String::from_utf8_lossy(&accumulated_stdout[tail_start..]);

                        if looks_like_interactive_prompt(&tail) {
                            return StallResult::Stalled(tail.to_string());
                        }
                    }
                }
            }
        }
    }

    async fn execute_background(&self, command: &str) -> Result<ToolResult> {
        let _child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        Ok(ToolResult::success(format!(
            "Command started in background: {}",
            command
        )))
    }

    fn format_output(&self, command: &str, working_dir: &Path, output: std::process::Output) -> Result<ToolResult> {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        tracing::debug!(
            command = %command,
            exit_code = exit_code,
            stdout_len = stdout.len(),
            stderr_len = stderr.len(),
            "Command completed"
        );

        let mut combined = String::new();

        if !stdout.is_empty() {
            combined.push_str(&stdout);
        }

        if !stderr.is_empty() {
            if !combined.is_empty() {
                combined.push('\n');
            }
            combined.push_str("[stderr]\n");
            combined.push_str(&stderr);
        }

        if !output.status.success() {
            if !combined.is_empty() {
                combined.push('\n');
            }
            combined.push_str(&format!("[exit code: {}]", exit_code));
            return Ok(ToolResult::error(combined));
        }

        let mut metadata = json!({
            "command": command,
            "cwd": working_dir.display().to_string(),
        });
        let cmd_base = command.split_whitespace().next().unwrap_or("");
        
        let cmd_type = if ["grep", "rg", "find", "ag", "ack"].contains(&cmd_base) {
            "search"
        } else if ["ls", "tree", "du"].contains(&cmd_base) {
            "list"
        } else if ["cat", "head", "tail", "less", "more"].contains(&cmd_base) {
            "read"
        } else {
            "generic"
        };
        metadata["command_type"] = json!(cmd_type);

        Ok(ToolResult::success_with_metadata(combined, metadata))
    }
}

/// Check if the tail of output looks like the command is waiting for interactive input.
fn looks_like_interactive_prompt(tail: &str) -> bool {
    let trimmed = tail.trim_end();
    if trimmed.is_empty() {
        return false;
    }

    // Check last line against known prompt patterns
    let last_line = trimmed.lines().last().unwrap_or("");
    for pattern in INTERACTIVE_PROMPT_PATTERNS {
        if last_line.contains(pattern) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_looks_like_interactive_prompt() {
        assert!(looks_like_interactive_prompt("Enter password: "));
        assert!(looks_like_interactive_prompt("Continue? [y/n] "));
        assert!(looks_like_interactive_prompt("Are you sure you want to proceed?"));
        assert!(looks_like_interactive_prompt("Username: "));
        assert!(!looks_like_interactive_prompt("Build completed successfully"));
        assert!(!looks_like_interactive_prompt(""));
        assert!(!looks_like_interactive_prompt("  \n  \n"));
    }

    #[tokio::test]
    async fn test_bash_simple_command() {
        let tool = BashTool;
        let params = json!({"command": "echo hello"});
        let result = tool.execute(params, &ToolContext::empty()).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("hello"));
    }

    #[tokio::test]
    async fn test_bash_failing_command() {
        let tool = BashTool;
        let params = json!({"command": "exit 1"});
        let result = tool.execute(params, &ToolContext::empty()).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("exit code: 1"));
    }

    #[tokio::test]
    async fn test_bash_timeout() {
        let tool = BashTool;
        let params = json!({"command": "sleep 10", "timeout": 1});
        let result = tool.execute(params, &ToolContext::empty()).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("timed out") || result.content.contains("Timeout"));
    }

    #[tokio::test]
    async fn test_bash_stderr() {
        let tool = BashTool;
        let params = json!({"command": "echo err >&2"});
        let result = tool.execute(params, &ToolContext::empty()).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("err"));
    }

    #[tokio::test]
    async fn test_bash_background() {
        let tool = BashTool;
        let params = json!({"command": "sleep 0.1", "run_in_background": true});
        let result = tool.execute(params, &ToolContext::empty()).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("background"));
    }
}
