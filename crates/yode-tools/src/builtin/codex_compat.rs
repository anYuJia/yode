use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::builtin::bash::BashTool;
use crate::builtin::shell_runtime::timeout_ms_description;
use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct ExecCommandTool;
pub struct ShellCommandTool;

#[async_trait]
impl Tool for ExecCommandTool {
    fn name(&self) -> &str {
        "exec_command"
    }

    fn user_facing_name(&self) -> &str {
        "Exec Command"
    }

    fn activity_description(&self, params: &Value) -> String {
        let command = params.get("cmd").and_then(Value::as_str).unwrap_or("");
        format!("Running command: {}", command)
    }

    fn description(&self) -> &str {
        "Codex-compatible shell command tool. Runs a command and returns output. Use this when Codex-style prompts call for exec_command; Yode executes it through the same runtime and safety checks as bash."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "cmd": {
                    "type": "string",
                    "description": "Shell command to execute."
                },
                "workdir": {
                    "type": "string",
                    "description": "Working directory for the command. Defaults to the current session working directory."
                },
                "yield_time_ms": {
                    "type": "integer",
                    "description": "Codex-compatible wait hint. Yode accepts it for compatibility; command output is returned when the command finishes unless run_in_background is true."
                },
                "max_output_tokens": {
                    "type": "integer",
                    "description": "Codex-compatible output budget hint. Yode may cap large command output using its normal shell output policy."
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": timeout_ms_description()
                },
                "run_in_background": {
                    "type": "boolean",
                    "default": false,
                    "description": "Whether to run the command in the background."
                }
            },
            "required": ["cmd"]
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
            .get("cmd")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: cmd"))?;

        let mut bash_params = json!({
            "command": command,
        });
        copy_if_present(&params, &mut bash_params, "timeout_ms");
        copy_if_present(&params, &mut bash_params, "run_in_background");

        let scoped_ctx = context_with_workdir(ctx, params.get("workdir").and_then(Value::as_str));
        BashTool.execute(bash_params, &scoped_ctx).await
    }
}

#[async_trait]
impl Tool for ShellCommandTool {
    fn name(&self) -> &str {
        "shell_command"
    }

    fn user_facing_name(&self) -> &str {
        "Shell Command"
    }

    fn activity_description(&self, params: &Value) -> String {
        let command = params
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("");
        format!("Running command: {}", command)
    }

    fn description(&self) -> &str {
        "Codex-compatible shell_command wrapper. Prefer Yode's bash tool when choosing directly; this exists so Codex-style tool calls keep working."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute."
                },
                "workdir": {
                    "type": "string",
                    "description": "Working directory for the command. Defaults to the current session working directory."
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": timeout_ms_description()
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
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: command"))?;

        let mut bash_params = json!({
            "command": command,
        });
        copy_if_present(&params, &mut bash_params, "timeout_ms");

        let scoped_ctx = context_with_workdir(ctx, params.get("workdir").and_then(Value::as_str));
        BashTool.execute(bash_params, &scoped_ctx).await
    }
}

fn copy_if_present(from: &Value, to: &mut Value, key: &str) {
    if let Some(value) = from.get(key) {
        to[key] = value.clone();
    }
}

fn context_with_workdir(ctx: &ToolContext, workdir: Option<&str>) -> ToolContext {
    let Some(workdir) = workdir.filter(|value| !value.trim().is_empty()) else {
        return ctx.clone();
    };
    let mut scoped = ctx.clone();
    let path = PathBuf::from(workdir);
    scoped.working_dir = Some(if path.is_absolute() {
        path
    } else if let Some(base) = &ctx.working_dir {
        base.join(path)
    } else {
        path
    });
    scoped
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::tool::{Tool, ToolContext};

    use super::{ExecCommandTool, ShellCommandTool};

    #[tokio::test]
    async fn exec_command_runs_codex_style_cmd() {
        let result = ExecCommandTool
            .execute(json!({ "cmd": "printf yode" }), &ToolContext::empty())
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("yode"));
    }

    #[tokio::test]
    async fn shell_command_uses_workdir() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::write(dir.path().join("marker.txt"), "ok")
            .await
            .unwrap();

        let result = ShellCommandTool
            .execute(
                json!({
                    "command": "pwd && ls marker.txt",
                    "workdir": dir.path().display().to_string()
                }),
                &ToolContext::empty(),
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("marker.txt"));
    }
}
