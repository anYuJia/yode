use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use base64::Engine;
use serde_json::{json, Value};

use crate::builtin::bash::BashTool;
use crate::builtin::shell_runtime::timeout_ms_description;
use crate::runtime_tasks::RuntimeTaskStatus;
use crate::state::TaskStatus;
use crate::tool::{
    Tool, ToolCapabilities, ToolContext, ToolErrorType, ToolResult, UserQuery, UserQueryOption,
    UserQuestion,
};

pub struct ExecCommandTool;
pub struct ShellCommandTool;
pub struct ApplyPatchTool;
pub struct ViewImageTool;
pub struct GetContextRemainingTool;
pub struct UpdatePlanTool;
pub struct WriteStdinTool;
pub struct RequestUserInputTool;

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
                "shell": {
                    "type": "string",
                    "description": "Optional shell binary to launch, for example bash, zsh, or sh. Defaults to Yode's standard shell runtime."
                },
                "login": {
                    "type": "boolean",
                    "description": "Whether to request login-shell semantics when shell is set. Accepted for Codex compatibility."
                },
                "tty": {
                    "type": "boolean",
                    "description": "Accepted for Codex compatibility. Yode background commands are pipe-based, not PTY-backed yet."
                },
                "run_in_background": {
                    "type": "boolean",
                    "default": false,
                    "description": "Whether to run the command in the background."
                },
                "sandbox_permissions": {
                    "type": "string",
                    "description": "Codex-compatible sandbox hint. Yode keeps its own permission and safety checks."
                },
                "justification": {
                    "type": "string",
                    "description": "User-facing approval justification when elevated permissions are requested."
                },
                "prefix_rule": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Codex-compatible approval prefix hint accepted for schema compatibility."
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
        let effective_command = effective_exec_command(command, &params);

        let mut bash_params = json!({
            "command": effective_command,
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
        let command = params.get("command").and_then(Value::as_str).unwrap_or("");
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

#[async_trait]
impl Tool for ApplyPatchTool {
    fn name(&self) -> &str {
        "apply_patch"
    }

    fn user_facing_name(&self) -> &str {
        "Apply Patch"
    }

    fn activity_description(&self, _params: &Value) -> String {
        "Applying patch".to_string()
    }

    fn description(&self) -> &str {
        r#"Apply a Codex-style patch to local files. Pass the full patch text in the `patch` field, including `*** Begin Patch` and `*** End Patch`.

This JSON wrapper exists because Yode currently exposes function tools; the accepted patch body follows Codex apply_patch syntax."#
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "patch": {
                    "type": "string",
                    "description": "Full Codex apply_patch text, including *** Begin Patch and *** End Patch."
                }
            },
            "required": ["patch"]
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
        let patch = params
            .get("patch")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: patch"))?;
        let cwd = ctx
            .working_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));
        apply_codex_patch(patch, &cwd).await
    }
}

#[async_trait]
impl Tool for ViewImageTool {
    fn name(&self) -> &str {
        "view_image"
    }

    fn user_facing_name(&self) -> &str {
        "View Image"
    }

    fn activity_description(&self, params: &Value) -> String {
        let path = params.get("path").and_then(Value::as_str).unwrap_or("");
        format!("Viewing image: {}", path)
    }

    fn description(&self) -> &str {
        "View a local image file from the filesystem when visual inspection is needed. Returns a data URL and image metadata for desktop/model integrations."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Local filesystem path to an image file."
                },
                "detail": {
                    "type": "string",
                    "description": "Image detail level. Defaults to high; original preserves exact bytes.",
                    "enum": ["high", "original"]
                }
            },
            "required": ["path"]
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
        let raw_path = params
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: path"))?;
        let detail = params
            .get("detail")
            .and_then(Value::as_str)
            .unwrap_or("high");
        if !matches!(detail, "high" | "original") {
            return Ok(ToolResult::error_typed(
                format!(
                    "view_image.detail only supports high or original, got {}",
                    detail
                ),
                ToolErrorType::Validation,
                true,
                Some("Use detail=\"high\" or detail=\"original\".".to_string()),
            ));
        }
        let path = resolve_path(ctx, raw_path);
        let metadata = match tokio::fs::metadata(&path).await {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => {
                return Ok(ToolResult::error_typed(
                    format!("Image path is not a file: {}", path.display()),
                    ToolErrorType::Validation,
                    true,
                    Some("Pass a file path, not a directory.".to_string()),
                ));
            }
            Err(error) => {
                return Ok(ToolResult::error_typed(
                    format!("Unable to locate image {}: {}", path.display(), error),
                    ToolErrorType::NotFound,
                    true,
                    Some("Check the image path and try again.".to_string()),
                ));
            }
        };
        let mime_type = image_mime_type(&path)
            .ok_or_else(|| anyhow::anyhow!("Unsupported image extension for {}", path.display()))?;
        let bytes = tokio::fs::read(&path).await?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let data_url = format!("data:{};base64,{}", mime_type, encoded);
        let content = format!(
            "Loaded image: {}\nMIME: {}\nBytes: {}\nDetail: {}",
            path.display(),
            mime_type,
            metadata.len(),
            detail
        );
        Ok(ToolResult::success_with_metadata(
            content,
            json!({
                "path": path.display().to_string(),
                "mime_type": mime_type,
                "byte_count": metadata.len(),
                "detail": detail,
                "image_url": data_url,
            }),
        ))
    }
}

#[async_trait]
impl Tool for GetContextRemainingTool {
    fn name(&self) -> &str {
        "get_context_remaining"
    }

    fn user_facing_name(&self) -> &str {
        "Context Remaining"
    }

    fn description(&self) -> &str {
        "Get the remaining tokens in the current context window."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: true,
        }
    }

    async fn execute(&self, _params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let tokens_left = match (ctx.context_window_tokens, ctx.estimated_context_tokens) {
            (Some(window), Some(used)) => Some(window.saturating_sub(used)),
            _ => None,
        };
        let content = match tokens_left {
            Some(tokens) => format!("Context remaining: {} tokens", tokens),
            None => "Context remaining: unknown".to_string(),
        };
        Ok(ToolResult::success_with_metadata(
            content,
            json!({
                "tokens_left": tokens_left,
                "context_window_tokens": ctx.context_window_tokens,
                "estimated_context_tokens": ctx.estimated_context_tokens,
            }),
        ))
    }
}

#[async_trait]
impl Tool for UpdatePlanTool {
    fn name(&self) -> &str {
        "update_plan"
    }

    fn user_facing_name(&self) -> &str {
        "Update Plan"
    }

    fn activity_description(&self, params: &Value) -> String {
        let count = params
            .get("plan")
            .and_then(Value::as_array)
            .map(|items| items.len())
            .unwrap_or(0);
        format!("Updating plan: {} step(s)", count)
    }

    fn description(&self) -> &str {
        r#"Updates the task plan.
Provide an optional explanation and a list of plan items, each with a step and status.
At most one step can be in_progress at a time."#
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "explanation": {
                    "type": "string",
                    "description": "Optional explanation for this plan update."
                },
                "plan": {
                    "type": "array",
                    "description": "The list of steps.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "step": {
                                "type": "string",
                                "description": "Task step text."
                            },
                            "status": {
                                "type": "string",
                                "description": "Step status: pending, in_progress, or completed."
                            }
                        },
                        "required": ["step", "status"]
                    }
                }
            },
            "required": ["plan"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let tasks = match &ctx.tasks {
            Some(tasks) => tasks,
            None => {
                return Ok(ToolResult::error_typed(
                    "Task store not available.".to_string(),
                    ToolErrorType::Execution,
                    true,
                    Some("Retry in an agent session with task store support.".to_string()),
                ));
            }
        };

        let plan = params
            .get("plan")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: plan"))?;

        let mut in_progress_count = 0usize;
        let mut parsed_steps = Vec::with_capacity(plan.len());
        for (index, item) in plan.iter().enumerate() {
            let step = item
                .get("step")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|step| !step.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!("plan[{}].step must be a non-empty string", index)
                })?;
            let status_label = item
                .get("status")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("plan[{}].status is required", index))?;
            let status = match status_label {
                "pending" => TaskStatus::Pending,
                "in_progress" => {
                    in_progress_count += 1;
                    TaskStatus::InProgress
                }
                "completed" => TaskStatus::Completed,
                other => {
                    return Ok(ToolResult::error_typed(
                        format!(
                            "Invalid status for plan[{}]: {}. Use pending, in_progress, or completed.",
                            index, other
                        ),
                        ToolErrorType::Validation,
                        true,
                        Some("Update the plan with valid Codex step statuses.".to_string()),
                    ));
                }
            };
            parsed_steps.push((step.to_string(), status));
        }

        if in_progress_count > 1 {
            return Ok(ToolResult::error_typed(
                "Invalid plan: at most one step can be in_progress.".to_string(),
                ToolErrorType::Validation,
                true,
                Some("Mark only the current step as in_progress.".to_string()),
            ));
        }

        let explanation = params
            .get("explanation")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let mut store = tasks.lock().await;
        store.clear();
        let mut saved = Vec::with_capacity(parsed_steps.len());
        for (step, status) in parsed_steps {
            let task = store.create(step, String::new());
            let id = task.id.clone();
            let task = store.update_status(&id, status).cloned().unwrap_or(task);
            saved.push(task);
        }

        let content = if saved.is_empty() {
            "Plan cleared.".to_string()
        } else {
            let mut lines = Vec::new();
            if let Some(explanation) = explanation.as_deref() {
                lines.push(explanation.to_string());
            }
            lines.extend(saved.iter().map(|task| {
                let marker = match task.status {
                    TaskStatus::Pending => "[ ]",
                    TaskStatus::InProgress => "[~]",
                    TaskStatus::Completed => "[x]",
                };
                format!("{} {}", marker, task.subject)
            }));
            lines.join("\n")
        };

        Ok(ToolResult::success_with_metadata(
            content,
            json!({
                "explanation": explanation,
                "plan": saved,
            }),
        ))
    }
}

#[async_trait]
impl Tool for WriteStdinTool {
    fn name(&self) -> &str {
        "write_stdin"
    }

    fn user_facing_name(&self) -> &str {
        "Write Stdin"
    }

    fn activity_description(&self, params: &Value) -> String {
        let target = params
            .get("session_id")
            .or_else(|| params.get("task_id"))
            .and_then(Value::as_str)
            .unwrap_or("latest");
        format!("Writing stdin: {}", target)
    }

    fn description(&self) -> &str {
        "Codex-compatible stdin writer for a running background command. Use this after starting exec_command/bash with run_in_background=true when the process is waiting for input."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "anyOf": [
                        { "type": "integer" },
                        { "type": "string" }
                    ],
                    "description": "Codex-compatible running command session id. Yode also accepts its runtime task id such as task-1."
                },
                "task_id": {
                    "type": "string",
                    "description": "Yode runtime task id. If omitted, uses the latest running task that accepts stdin."
                },
                "chars": {
                    "type": "string",
                    "description": "Bytes/text to write to stdin. Defaults to empty, which polls the running command without writing."
                },
                "yield_time_ms": {
                    "type": "integer",
                    "description": "Codex-compatible wait hint accepted for schema compatibility."
                },
                "max_output_tokens": {
                    "type": "integer",
                    "description": "Codex-compatible output budget hint accepted for schema compatibility."
                }
            },
            "required": []
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let Some(runtime_tasks) = &ctx.runtime_tasks else {
            return Ok(ToolResult::error_typed(
                "Runtime task store not available.".to_string(),
                ToolErrorType::Execution,
                true,
                Some("Start a background command in an agent session first.".to_string()),
            ));
        };
        let chars = params
            .get("chars")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let target = params
            .get("session_id")
            .or_else(|| params.get("task_id"))
            .and_then(runtime_task_id_from_value);

        let task_id = {
            let store = runtime_tasks.lock().await;
            if let Some(target) = target {
                normalize_runtime_task_id(&target, &store)
            } else {
                store
                    .list()
                    .into_iter()
                    .rev()
                    .find(|task| {
                        matches!(
                            task.status,
                            RuntimeTaskStatus::Pending | RuntimeTaskStatus::Running
                        )
                    })
                    .map(|task| task.id)
                    .ok_or_else(|| anyhow::anyhow!("No running runtime task found."))?
            }
        };

        if chars.is_empty() {
            let task = {
                let store = runtime_tasks.lock().await;
                store.get(&task_id)
            }
            .ok_or_else(|| anyhow::anyhow!("Runtime task '{}' not found.", task_id))?;
            let output_preview = tokio::fs::read_to_string(&task.output_path)
                .await
                .ok()
                .map(|output| tail_chars(&output, 4000));
            let content = match output_preview.as_deref() {
                Some(output) if !output.trim().is_empty() => output.to_string(),
                _ => format!("Runtime task '{}' status: {:?}", task_id, task.status),
            };
            return Ok(ToolResult::success_with_metadata(
                content,
                json!({
                    "task_id": task_id,
                    "session_id": task_id,
                    "status": task.status,
                    "output_path": task.output_path,
                }),
            ));
        }

        let mut last_error = None;
        for attempt in 0..20 {
            let write_result = runtime_tasks
                .lock()
                .await
                .write_stdin(&task_id, chars.clone());
            match write_result {
                Ok(()) => {
                    return Ok(ToolResult::success_with_metadata(
                        format!("Wrote {} byte(s) to {}.", chars.len(), task_id),
                        json!({
                            "task_id": task_id,
                            "session_id": task_id,
                            "bytes": chars.len(),
                        }),
                    ));
                }
                Err(error) if error.contains("does not accept stdin") && attempt < 19 => {
                    last_error = Some(error);
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
                Err(error) => {
                    last_error = Some(error);
                    break;
                }
            }
        }

        match last_error {
            None => Ok(ToolResult::success_with_metadata(
                format!("Wrote {} byte(s) to {}.", chars.len(), task_id),
                json!({
                    "task_id": task_id,
                    "session_id": task_id,
                    "bytes": chars.len(),
                }),
            )),
            Some(error) => Ok(ToolResult::error_typed(
                error,
                ToolErrorType::Execution,
                true,
                Some("Check task_output or /tasks to confirm the command is still running and accepts stdin.".to_string()),
            )),
        }
    }
}

#[async_trait]
impl Tool for RequestUserInputTool {
    fn name(&self) -> &str {
        "request_user_input"
    }

    fn user_facing_name(&self) -> &str {
        "Request User Input"
    }

    fn activity_description(&self, params: &Value) -> String {
        let first_q = params
            .get("questions")
            .and_then(Value::as_array)
            .and_then(|questions| questions.first())
            .and_then(|question| question.get("question"))
            .and_then(Value::as_str)
            .unwrap_or("questions");
        format!("Requesting user input: {}", first_q)
    }

    fn description(&self) -> &str {
        "Codex-compatible request_user_input tool. Ask the user one to three short multiple-choice questions and return answers mapped by question id."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "description": "Questions to show the user. Prefer 1 and do not exceed 3.",
                    "minItems": 1,
                    "maxItems": 3,
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {
                                "type": "string",
                                "description": "Stable identifier for mapping answers (snake_case)."
                            },
                            "header": {
                                "type": "string",
                                "description": "Short header label shown in the UI (12 or fewer chars)."
                            },
                            "question": {
                                "type": "string",
                                "description": "Single-sentence prompt shown to the user."
                            },
                            "options": {
                                "type": "array",
                                "description": "Provide 2-3 mutually exclusive choices. Put the recommended option first and suffix its label with (Recommended).",
                                "minItems": 2,
                                "maxItems": 3,
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "label": {
                                            "type": "string",
                                            "description": "User-facing label (1-5 words)."
                                        },
                                        "description": {
                                            "type": "string",
                                            "description": "One short sentence explaining impact/tradeoff if selected."
                                        }
                                    },
                                    "required": ["label", "description"]
                                }
                            }
                        },
                        "required": ["id", "header", "question", "options"]
                    }
                },
                "autoResolutionMs": {
                    "type": "integer",
                    "description": "Codex-compatible optional auto-resolution window. Accepted for schema compatibility; Yode currently waits for an explicit answer."
                }
            },
            "required": ["questions"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: false,
            read_only: true,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let question_values = params
            .get("questions")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: questions"))?;
        if question_values.is_empty() || question_values.len() > 3 {
            return Ok(ToolResult::error_typed(
                "request_user_input requires one to three questions.".to_string(),
                ToolErrorType::Validation,
                true,
                Some("Provide 1-3 short questions.".to_string()),
            ));
        }

        let mut id_by_key = BTreeMap::new();
        let mut questions = Vec::with_capacity(question_values.len());
        for (index, question_value) in question_values.iter().enumerate() {
            let id = required_string(question_value, "id")
                .map(str::to_string)
                .map_err(|error| anyhow::anyhow!("questions[{}].{}", index, error))?;
            let header = required_string(question_value, "header")
                .map(str::to_string)
                .map_err(|error| anyhow::anyhow!("questions[{}].{}", index, error))?;
            let question = required_string(question_value, "question")
                .map(str::to_string)
                .map_err(|error| anyhow::anyhow!("questions[{}].{}", index, error))?;
            let options_value = question_value
                .get("options")
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow::anyhow!("questions[{}].options must be an array", index))?;
            if options_value.is_empty() {
                return Ok(ToolResult::error_typed(
                    format!("questions[{}].options must not be empty.", index),
                    ToolErrorType::Validation,
                    true,
                    Some("Provide 2-3 options for each question.".to_string()),
                ));
            }

            let mut options = Vec::with_capacity(options_value.len());
            for option in options_value {
                options.push(UserQueryOption {
                    label: option
                        .get("label")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    description: option
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    preview: None,
                });
            }

            id_by_key.insert(header.clone(), id.clone());
            id_by_key.insert(question.clone(), id);
            questions.push(UserQuestion {
                question,
                header,
                options,
                multi_select: false,
            });
        }

        let tx = match &ctx.user_input_tx {
            Some(tx) => tx,
            None => {
                return Ok(ToolResult::error_typed(
                    "User input channel not available.".to_string(),
                    ToolErrorType::Execution,
                    true,
                    Some("Retry in a session with interactive user input support.".to_string()),
                ));
            }
        };
        let rx = match &ctx.user_input_rx {
            Some(rx) => rx,
            None => {
                return Ok(ToolResult::error_typed(
                    "User input response channel not available.".to_string(),
                    ToolErrorType::Execution,
                    true,
                    Some("Retry in a session with interactive user input support.".to_string()),
                ));
            }
        };

        let query_id = uuid::Uuid::new_v4().to_string();
        if let Err(error) = tx.send(UserQuery {
            id: query_id,
            questions,
        }) {
            return Ok(ToolResult::error_typed(
                format!("Failed to send request_user_input query: {}", error),
                ToolErrorType::Execution,
                true,
                Some("Retry after the UI is ready for user input.".to_string()),
            ));
        }

        let raw_answers = {
            let mut guard = rx.lock().await;
            guard.recv().await
        };
        let Some(raw_answers) = raw_answers else {
            return Ok(ToolResult::error_typed(
                "User input channel closed.".to_string(),
                ToolErrorType::Execution,
                true,
                Some("Retry after reconnecting the UI session.".to_string()),
            ));
        };

        let answers = codex_request_user_input_answers(&raw_answers, &id_by_key);
        Ok(ToolResult::success_with_metadata(
            format!("User answered: {}", serde_json::to_string(&answers)?),
            json!({
                "answers": answers,
                "raw_answers": raw_answers,
            }),
        ))
    }
}

fn copy_if_present(from: &Value, to: &mut Value, key: &str) {
    if let Some(value) = from.get(key) {
        to[key] = value.clone();
    }
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{} must be a non-empty string", key))
}

fn codex_request_user_input_answers(
    raw_answers: &str,
    id_by_key: &BTreeMap<String, String>,
) -> Value {
    let mut answers = serde_json::Map::new();
    let Ok(Value::Object(raw_map)) = serde_json::from_str::<Value>(raw_answers) else {
        return Value::Object(answers);
    };

    for (key, value) in raw_map {
        let id = id_by_key.get(&key).cloned().unwrap_or(key);
        let values = match value {
            Value::Array(items) => items
                .into_iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect::<Vec<_>>(),
            Value::String(item) => vec![item],
            other => vec![other.to_string()],
        };
        answers.insert(id, json!({ "answers": values }));
    }

    Value::Object(answers)
}

fn runtime_task_id_from_value(value: &Value) -> Option<String> {
    match value {
        Value::String(raw) => {
            let trimmed = raw.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        Value::Number(number) => number
            .as_u64()
            .map(|id| format!("task-{}", id))
            .or_else(|| number.as_i64().map(|id| format!("task-{}", id))),
        _ => None,
    }
}

fn normalize_runtime_task_id(
    candidate: &str,
    store: &crate::runtime_tasks::RuntimeTaskStore,
) -> String {
    if store.get(candidate).is_some() {
        return candidate.to_string();
    }
    if candidate.chars().all(|ch| ch.is_ascii_digit()) {
        let task_id = format!("task-{}", candidate);
        if store.get(&task_id).is_some() {
            return task_id;
        }
    }
    candidate.to_string()
}

fn tail_chars(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value.to_string();
    }
    value
        .chars()
        .skip(char_count.saturating_sub(max_chars))
        .collect()
}

fn effective_exec_command(command: &str, params: &Value) -> String {
    let Some(shell) = params
        .get("shell")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return command.to_string();
    };

    let flag = if params
        .get("login")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "-lc"
    } else {
        "-c"
    };
    format!("{} {} {}", shell_quote(shell), flag, shell_quote(command))
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

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

async fn apply_codex_patch(patch: &str, cwd: &std::path::Path) -> Result<ToolResult> {
    let lines = patch.lines().collect::<Vec<_>>();
    if lines.first() != Some(&"*** Begin Patch") || lines.last() != Some(&"*** End Patch") {
        return Ok(ToolResult::error_typed(
            "Invalid patch: expected *** Begin Patch and *** End Patch markers.".to_string(),
            ToolErrorType::Validation,
            true,
            Some("Pass the complete Codex apply_patch body.".to_string()),
        ));
    }

    let mut index = 1usize;
    let mut changed = Vec::new();
    while index + 1 < lines.len() {
        let line = lines[index];
        if let Some(path) = line.strip_prefix("*** Add File: ") {
            index += 1;
            let mut content = String::new();
            while index + 1 < lines.len() && !lines[index].starts_with("*** ") {
                let Some(rest) = lines[index].strip_prefix('+') else {
                    return Ok(invalid_patch(format!(
                        "Add File line must start with '+': {}",
                        lines[index]
                    )));
                };
                content.push_str(rest);
                content.push('\n');
                index += 1;
            }
            let target = cwd.join(path);
            if let Some(parent) = target.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(&target, content).await?;
            changed.push(path.to_string());
            continue;
        }
        if let Some(path) = line.strip_prefix("*** Delete File: ") {
            let target = cwd.join(path);
            tokio::fs::remove_file(&target).await?;
            changed.push(path.to_string());
            index += 1;
            continue;
        }
        if let Some(path) = line.strip_prefix("*** Update File: ") {
            index += 1;
            let mut move_to = None;
            if index + 1 < lines.len() {
                if let Some(dest) = lines[index].strip_prefix("*** Move to: ") {
                    move_to = Some(dest.to_string());
                    index += 1;
                }
            }
            let mut old = String::new();
            let mut new = String::new();
            while index + 1 < lines.len()
                && (!lines[index].starts_with("*** ") || lines[index] == "*** End of File")
            {
                let current = lines[index];
                if current.starts_with("@@") {
                    index += 1;
                    continue;
                }
                if current == "*** End of File" {
                    index += 1;
                    continue;
                }
                let Some(marker) = current.chars().next() else {
                    index += 1;
                    continue;
                };
                let body = &current[marker.len_utf8()..];
                match marker {
                    ' ' => {
                        old.push_str(body);
                        old.push('\n');
                        new.push_str(body);
                        new.push('\n');
                    }
                    '-' => {
                        old.push_str(body);
                        old.push('\n');
                    }
                    '+' => {
                        new.push_str(body);
                        new.push('\n');
                    }
                    _ => return Ok(invalid_patch(format!("Invalid update line: {}", current))),
                }
                index += 1;
            }
            let target = cwd.join(path);
            let content = tokio::fs::read_to_string(&target).await?;
            let updated = replace_patch_chunk(&content, &old, &new).ok_or_else(|| {
                anyhow::anyhow!("Patch context not found in {}", target.display())
            })?;
            if let Some(dest) = move_to {
                let dest_path = cwd.join(&dest);
                if let Some(parent) = dest_path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                tokio::fs::write(&dest_path, updated).await?;
                tokio::fs::remove_file(&target).await?;
                changed.push(format!("{} -> {}", path, dest));
            } else {
                tokio::fs::write(&target, updated).await?;
                changed.push(path.to_string());
            }
            continue;
        }
        return Ok(invalid_patch(format!(
            "Unsupported patch directive: {}",
            line
        )));
    }

    Ok(ToolResult::success_with_metadata(
        format!("Applied patch to {} file(s).", changed.len()),
        json!({ "changed_files": changed }),
    ))
}

fn replace_patch_chunk(content: &str, old: &str, new: &str) -> Option<String> {
    if old.is_empty() {
        return None;
    }
    if let Some(updated) = replace_once(content, old, new) {
        return Some(updated);
    }
    let trimmed_old = old.strip_suffix('\n').unwrap_or(old);
    let trimmed_new = new.strip_suffix('\n').unwrap_or(new);
    replace_once(content, trimmed_old, trimmed_new)
}

fn replace_once(content: &str, old: &str, new: &str) -> Option<String> {
    let index = content.find(old)?;
    let mut updated = String::with_capacity(content.len() - old.len() + new.len());
    updated.push_str(&content[..index]);
    updated.push_str(new);
    updated.push_str(&content[index + old.len()..]);
    Some(updated)
}

fn invalid_patch(message: String) -> ToolResult {
    ToolResult::error_typed(
        message,
        ToolErrorType::Validation,
        true,
        Some("Use Codex apply_patch syntax and include enough unchanged context.".to_string()),
    )
}

fn resolve_path(ctx: &ToolContext, raw_path: &str) -> PathBuf {
    let path = PathBuf::from(raw_path);
    if path.is_absolute() {
        path
    } else if let Some(cwd) = &ctx.working_dir {
        cwd.join(path)
    } else {
        path
    }
}

fn image_mime_type(path: &std::path::Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => Some("image/png"),
        Some("jpg") | Some("jpeg") => Some("image/jpeg"),
        Some("gif") => Some("image/gif"),
        Some("webp") => Some("image/webp"),
        Some("bmp") => Some("image/bmp"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::builtin::task_output::TaskOutputTool;
    use crate::runtime_tasks::RuntimeTaskStore;
    use crate::tool::{Tool, ToolContext};

    use super::{
        ApplyPatchTool, ExecCommandTool, GetContextRemainingTool, ShellCommandTool, UpdatePlanTool,
        ViewImageTool, WriteStdinTool, RequestUserInputTool,
    };

    #[tokio::test]
    async fn exec_command_runs_codex_style_cmd() {
        let result = ExecCommandTool
            .execute(json!({ "cmd": "printf yode" }), &ToolContext::empty())
            .await
            .unwrap();

        assert!(!result.is_error, "{}", result.content);
        assert!(result.content.contains("yode"));
    }

    #[tokio::test]
    async fn exec_command_accepts_codex_shell_options() {
        let result = ExecCommandTool
            .execute(
                json!({
                    "cmd": "printf shell-ok",
                    "shell": "sh",
                    "login": false,
                    "tty": false,
                    "sandbox_permissions": "use_default",
                    "justification": "test",
                    "prefix_rule": ["printf"]
                }),
                &ToolContext::empty(),
            )
            .await
            .unwrap();

        assert!(!result.is_error, "{}", result.content);
        assert!(result.content.contains("shell-ok"));
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

        assert!(!result.is_error, "{}", result.content);
        assert!(result.content.contains("marker.txt"));
    }

    #[tokio::test]
    async fn apply_patch_updates_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("hello.txt");
        tokio::fs::write(&file, "hello\nworld\n").await.unwrap();
        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());

        let result = ApplyPatchTool
            .execute(
                json!({
                    "patch": "*** Begin Patch\n*** Update File: hello.txt\n@@\n hello\n-world\n+yode\n*** End Patch"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error, "{}", result.content);
        assert_eq!(
            tokio::fs::read_to_string(&file).await.unwrap(),
            "hello\nyode\n"
        );
    }

    #[tokio::test]
    async fn apply_patch_accepts_end_of_file_marker() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("hello.txt");
        tokio::fs::write(&file, "hello\nworld\n").await.unwrap();
        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());

        let result = ApplyPatchTool
            .execute(
                json!({
                    "patch": "*** Begin Patch\n*** Update File: hello.txt\n@@\n hello\n-world\n+yode\n*** End of File\n*** End Patch"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error, "{}", result.content);
        assert_eq!(
            tokio::fs::read_to_string(&file).await.unwrap(),
            "hello\nyode\n"
        );
    }

    #[tokio::test]
    async fn view_image_returns_data_url_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("tiny.png");
        tokio::fs::write(&image, b"fake").await.unwrap();
        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());

        let result = ViewImageTool
            .execute(json!({ "path": "tiny.png" }), &ctx)
            .await
            .unwrap();

        assert!(!result.is_error);
        assert_eq!(
            result.metadata.as_ref().unwrap()["mime_type"],
            json!("image/png")
        );
        assert!(result.metadata.as_ref().unwrap()["image_url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,"));
    }

    #[tokio::test]
    async fn get_context_remaining_uses_context_metrics() {
        let mut ctx = ToolContext::empty();
        ctx.context_window_tokens = Some(100);
        ctx.estimated_context_tokens = Some(35);

        let result = GetContextRemainingTool
            .execute(json!({}), &ctx)
            .await
            .unwrap();

        assert!(!result.is_error);
        assert_eq!(result.metadata.as_ref().unwrap()["tokens_left"], json!(65));
    }

    #[tokio::test]
    async fn update_plan_replaces_task_store() {
        let tasks = std::sync::Arc::new(tokio::sync::Mutex::new(crate::state::TaskStore::new()));
        let mut ctx = ToolContext::empty();
        ctx.tasks = Some(tasks.clone());

        let result = UpdatePlanTool
            .execute(
                json!({
                    "explanation": "梳理下一步",
                    "plan": [
                        { "step": "检查工具列表", "status": "completed" },
                        { "step": "补齐兼容工具", "status": "in_progress" },
                        { "step": "运行测试", "status": "pending" }
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error, "{}", result.content);
        assert!(result.content.contains("[~] 补齐兼容工具"));
        let store = tasks.lock().await;
        let all = store.list();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].status, crate::state::TaskStatus::Completed);
        assert_eq!(all[1].status, crate::state::TaskStatus::InProgress);
        assert_eq!(all[2].status, crate::state::TaskStatus::Pending);
    }

    #[tokio::test]
    async fn update_plan_rejects_multiple_in_progress_steps() {
        let tasks = std::sync::Arc::new(tokio::sync::Mutex::new(crate::state::TaskStore::new()));
        let mut ctx = ToolContext::empty();
        ctx.tasks = Some(tasks);

        let result = UpdatePlanTool
            .execute(
                json!({
                    "plan": [
                        { "step": "one", "status": "in_progress" },
                        { "step": "two", "status": "in_progress" }
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.is_error);
        assert!(result.content.contains("at most one step"));
    }

    #[tokio::test]
    async fn write_stdin_feeds_background_exec_command() {
        let dir = tempfile::tempdir().unwrap();
        let runtime_tasks = std::sync::Arc::new(tokio::sync::Mutex::new(RuntimeTaskStore::new()));
        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());
        ctx.runtime_tasks = Some(runtime_tasks);

        let started = ExecCommandTool
            .execute(
                json!({
                    "cmd": "printf 'ready\\n'; IFS= read line; printf 'got:%s\\n' \"$line\"",
                    "run_in_background": true
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!started.is_error, "{}", started.content);
        let task_id = started.metadata.as_ref().unwrap()["task_id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(
            started.metadata.as_ref().unwrap()["session_id"].as_str(),
            Some(task_id.as_str())
        );

        let wrote = WriteStdinTool
            .execute(
                json!({
                    "session_id": task_id,
                    "chars": "hello from stdin\n"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!wrote.is_error, "{}", wrote.content);

        let output = TaskOutputTool
            .execute(
                json!({
                    "task_id": task_id,
                    "block": true,
                    "timeout": 5000
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!output.is_error, "{}", output.content);
        assert!(output.content.contains("ready"));
        assert!(output.content.contains("got:hello from stdin"));
    }

    #[tokio::test]
    async fn write_stdin_accepts_codex_numeric_session_id_and_empty_poll() {
        let dir = tempfile::tempdir().unwrap();
        let runtime_tasks = std::sync::Arc::new(tokio::sync::Mutex::new(RuntimeTaskStore::new()));
        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());
        ctx.runtime_tasks = Some(runtime_tasks);

        let started = ExecCommandTool
            .execute(
                json!({
                    "cmd": "printf 'ready\\n'; IFS= read line; printf 'got:%s\\n' \"$line\"",
                    "run_in_background": true
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!started.is_error, "{}", started.content);

        let wrote = WriteStdinTool
            .execute(
                json!({
                    "session_id": 1,
                    "chars": "numeric id\n"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!wrote.is_error, "{}", wrote.content);

        let polled = WriteStdinTool
            .execute(json!({ "session_id": 1 }), &ctx)
            .await
            .unwrap();
        assert!(!polled.is_error, "{}", polled.content);
        assert_eq!(
            polled.metadata.as_ref().unwrap()["task_id"].as_str(),
            Some("task-1")
        );

        let output = TaskOutputTool
            .execute(
                json!({
                    "task_id": "task-1",
                    "block": true,
                    "timeout": 5000
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!output.is_error, "{}", output.content);
        assert!(output.content.contains("got:numeric id"));
    }

    #[tokio::test]
    async fn request_user_input_maps_answers_by_question_id() {
        let (query_tx, mut query_rx) = tokio::sync::mpsc::unbounded_channel();
        let (answer_tx, answer_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut ctx = ToolContext::empty();
        ctx.user_input_tx = Some(query_tx);
        ctx.user_input_rx = Some(std::sync::Arc::new(tokio::sync::Mutex::new(answer_rx)));

        let handle = tokio::spawn(async move {
            RequestUserInputTool
                .execute(
                    json!({
                        "questions": [{
                            "id": "confirm_path",
                            "header": "路径",
                            "question": "使用这个路径吗？",
                            "options": [
                                { "label": "是 (Recommended)", "description": "继续使用当前路径。" },
                                { "label": "否", "description": "先调整路径。" }
                            ]
                        }]
                    }),
                    &ctx,
                )
                .await
                .unwrap()
        });

        let query = query_rx.recv().await.unwrap();
        assert_eq!(query.questions.len(), 1);
        assert_eq!(query.questions[0].header, "路径");
        answer_tx
            .send("{\"路径\":\"是 (Recommended)\"}".to_string())
            .unwrap();

        let result = handle.await.unwrap();
        assert!(!result.is_error, "{}", result.content);
        assert_eq!(
            result.metadata.as_ref().unwrap()["answers"]["confirm_path"]["answers"],
            json!(["是 (Recommended)"])
        );
    }
}
