use std::process::Stdio;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult, ToolErrorType};

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

    fn description(&self) -> &str {
        "Execute a shell command via sh -c. Captures stdout and stderr with a configurable timeout. \
         Includes a stall watchdog that detects when commands are blocked waiting for interactive input."
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

    fn requires_confirmation(&self) -> bool {
        true
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let command = params
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: command"))?;

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
            .spawn()?;

        // Stall watchdog: periodically check if the command is stalled
        let stall_check = self.run_with_stall_watchdog(&mut child, timeout_duration).await;

        match stall_check {
            StallResult::Completed(output) => self.format_output(command, output),
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
                            accumulated_stdout.extend_from_slice(&buf[..n]);
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

    fn format_output(&self, command: &str, output: std::process::Output) -> Result<ToolResult> {
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

        Ok(ToolResult::success(combined))
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
