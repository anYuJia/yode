mod background;
mod output;
mod watchdog;

use std::path::Path;
use std::process::Stdio;
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use serde_json::{json, Value};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use crate::builtin::shell_runtime::{
    command_timeout_secs, timeout_ms_description, GitChangeSnapshot,
};
use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolErrorType, ToolProgress, ToolResult};

const STALL_CHECK_INTERVAL_MS: u64 = 5_000;
const STALL_THRESHOLD_MS: u64 = 45_000;
const STALL_TAIL_BYTES: usize = 1024;
const DESTRUCTIVE_COMMAND_PATTERNS: &[&str] = &[
    r"(?i)(^|[;&|]\s*)rm\s+(-[^\s]*r[^\s]*f|- [^\n;]*r[^\n;]*f)[^\n;]*(\s|=)/(?:\s|$|[;&|])",
    r"(?i)(^|[;&|]\s*)rm\s+(-[^\s]*r[^\s]*f|- [^\n;]*r[^\n;]*f)[^\n;]*(/System|/Library|/usr|/bin|/sbin|/etc|/var)(?:\s|$|[;&|])",
    r"(?i)(^|[;&|]\s*)mkfs(\.|[\s])",
    r"(?i)(^|[;&|]\s*)dd\s+[^;&|]*(of=|if=)/dev/(sd|nvme|disk|rdisk)",
    r"(?i)>\s*/dev/(sd|nvme|disk|rdisk)",
    r"(?i)(^|[;&|]\s*)chmod\s+-R\s+777\s+/",
    r"(?i)(curl|wget)\b[^;&|]*\|\s*(sudo\s+)?(sh|bash)\b",
    r"(?i)(^|[;&|]\s*)git\s+reset\s+--hard(?:\s|$)",
    r"(?i)(^|[;&|]\s*)git\s+clean\s+-[^\s]*[fxd]",
];

const INTERACTIVE_PROMPT_PATTERNS: &[&str] = &[
    "password:", "Password:", "passphrase", "[y/n]", "[Y/n]", "[yes/no]",
    "Are you sure", "are you sure", "Continue?", "continue?", "Press any key",
    "press any key", "Enter ", "enter ", "Username:", "username:", "(yes/no)",
    "(Y/N)", "Do you want to", "do you want to", "> ", "$ ", "# ",
];

static DESTRUCTIVE_COMMAND_REGEXES: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    DESTRUCTIVE_COMMAND_PATTERNS
        .iter()
        .map(|pattern| Regex::new(pattern).expect("destructive bash command pattern should compile"))
        .collect()
});

pub struct BashTool;

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str { "bash" }

    fn user_facing_name(&self) -> &str { "Bash" }

    fn activity_description(&self, params: &Value) -> String {
        let command = params.get("command").and_then(|v| v.as_str()).unwrap_or("");
        format!("Running command: {}", command)
    }

    fn description(&self) -> &str {
        r#"Executes a given bash command and returns its output.

The working directory persists between commands, but shell state does not. Commands are executed through Yode's OS sandbox by default when a supported backend is available.

IMPORTANT: Avoid using this tool to run `find`, `grep`, `cat`, `head`, `tail`, `sed`, `awk`, or `echo` commands, unless explicitly instructed or after you have verified that a dedicated tool cannot accomplish your task. Instead, use the appropriate dedicated tool:
- File search: Use `glob`
- Content search: Use `grep`
- Read files: Use `read_file`
- Edit files: Use `edit_file`
- Write files: Use `write_file`
- Communication: Output text directly

# Instructions
- If your command will create new directories or files, first use `ls` to verify the parent directory exists and is correct.
- Always quote file paths that contain spaces.
- Prefer absolute paths and avoid changing the working directory unless needed.
- Timeout defaults to 120000ms and may be raised to 600000ms.
- Use `run_in_background` only when the result is not immediately required.
- Independent commands should be separate parallel tool calls; dependent commands should use `&&`.
- Prefer dedicated Git tools and reversible operations. Never bypass hooks unless explicitly requested.
- The sandbox is controlled by `YODE_SANDBOX_MODE=auto|strict|off` and `YODE_SANDBOX_NETWORK=inherit|deny`. `strict` fails closed if no supported OS backend exists.
- `dangerously_disable_sandbox` must only be used after sandbox restrictions are proven to block a legitimate command; Yode's destructive-command guard still remains active.
"#
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The bash command to execute" },
                "description": { "type": "string", "description": "A short (3-5 word) description of the task being performed" },
                "run_in_background": { "type": "boolean", "default": false, "description": "Whether to run the command in the background." },
                "timeout_ms": { "type": "integer", "description": timeout_ms_description() },
                "dangerously_disable_sandbox": { "type": "boolean", "default": false, "description": "Disable the OS sandbox for this command after a proven sandbox incompatibility. The destructive-command guard remains active." }
            },
            "required": ["command"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities { requires_confirmation: true, supports_auto_execution: false, read_only: false }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let command = params
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: command"))?;
        let working_dir = ctx.working_dir.as_deref().unwrap_or_else(|| Path::new("."));
        let dangerously_disable_sandbox = params
            .get("dangerously_disable_sandbox")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if dangerously_disable_sandbox {
            tracing::warn!("bash OS sandbox explicitly disabled; destructive command guard remains active");
        }
        if let Some(reason) = destructive_command_reason(command) {
            return Ok(ToolResult::error_typed(
                format!("Refusing to run potentially destructive bash command: {}\nCommand: {}", reason, command),
                ToolErrorType::Permission,
                false,
                Some("Use a narrower, reversible command or ask the user for an explicit manual recovery action.".to_string()),
            ));
        }

        let timeout_secs = command_timeout_secs(&params);
        let run_in_background = params
            .get("run_in_background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        tracing::debug!(command = %command, timeout_secs, background = run_in_background, "Executing bash command");

        if run_in_background {
            return self
                .execute_background(command, working_dir, ctx, dangerously_disable_sandbox)
                .await;
        }

        let before_changes = GitChangeSnapshot::capture(working_dir).await;
        let timeout_duration = Duration::from_secs(timeout_secs);
        let prepared = crate::sandbox::prepare_shell(command, working_dir, dangerously_disable_sandbox)?;
        let sandbox_info = prepared.info.clone();
        let mut cmd = Command::new(&prepared.executable);
        cmd.args(&prepared.args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(working_dir);
        crate::process_env::spawn_in_new_process_group(&mut cmd);
        let mut child = cmd.spawn()?;

        let stall_check = self
            .run_with_stall_watchdog(&mut child, timeout_duration, ctx.progress_tx.clone())
            .await;

        let mut result = match stall_check {
            watchdog::StallResult::Completed(output) => {
                let modified_files = if output.status.success() {
                    if let Some(before) = before_changes.as_ref() {
                        before.changed_files_since(working_dir).await
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };
                self.format_output(command, working_dir, output, modified_files).await?
            }
            watchdog::StallResult::Stalled(partial_output) => {
                kill_child_after_bash_interruption(&mut child, command, "stalled").await;
                ToolResult::error_typed(
                    format!("Command appears to be stalled (waiting for interactive input).\nLast output:\n{}\n\nThe command was killed. Add non-interactive flags or pipe explicit input.", partial_output),
                    ToolErrorType::Timeout,
                    true,
                    Some("Add non-interactive flags or pipe input to avoid stalling.".to_string()),
                )
            }
            watchdog::StallResult::Timeout => {
                kill_child_after_bash_interruption(&mut child, command, "timeout").await;
                ToolResult::error_typed(
                    format!("Command timed out after {} seconds", timeout_secs),
                    ToolErrorType::Timeout,
                    true,
                    Some("Increase timeout or reduce scope.".to_string()),
                )
            }
            watchdog::StallResult::Error(e) => {
                kill_child_after_bash_interruption(&mut child, command, "watchdog_error").await;
                ToolResult::error(format!("Failed to execute command: {}", e))
            }
        };
        crate::sandbox::annotate_tool_result(&mut result, &sandbox_info);
        Ok(result)
    }
}

async fn kill_child_after_bash_interruption(child: &mut Child, command: &str, reason: &str) {
    if let Err(err) = crate::process_env::kill_process_group(child).await {
        tracing::warn!(command = %command, reason, error = %err, "Failed to kill interrupted bash command");
    }
}

fn destructive_command_reason(command: &str) -> Option<&'static str> {
    for pattern in DESTRUCTIVE_COMMAND_REGEXES.iter() {
        if pattern.is_match(command) {
            return Some("matches destructive command guard");
        }
    }
    None
}

#[cfg(test)]
mod tests;
