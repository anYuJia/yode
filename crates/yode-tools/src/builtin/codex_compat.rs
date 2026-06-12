use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use base64::Engine;
use serde_json::{json, Value};

use crate::builtin::bash::BashTool;
use crate::builtin::shell_runtime::timeout_ms_description;
use crate::state::TaskStatus;
use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolErrorType, ToolResult};

pub struct ExecCommandTool;
pub struct ShellCommandTool;
pub struct ApplyPatchTool;
pub struct ViewImageTool;
pub struct GetContextRemainingTool;
pub struct UpdatePlanTool;

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
        let cwd = ctx.working_dir.clone().unwrap_or_else(|| PathBuf::from("."));
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
                format!("view_image.detail only supports high or original, got {}", detail),
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
        let mime_type = image_mime_type(&path).ok_or_else(|| {
            anyhow::anyhow!("Unsupported image extension for {}", path.display())
        })?;
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
                .ok_or_else(|| anyhow::anyhow!("plan[{}].step must be a non-empty string", index))?;
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
            let task = store
                .update_status(&id, status)
                .cloned()
                .unwrap_or(task);
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
        return Ok(invalid_patch(format!("Unsupported patch directive: {}", line)));
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

    use crate::tool::{Tool, ToolContext};

    use super::{
        ApplyPatchTool, ExecCommandTool, GetContextRemainingTool, ShellCommandTool, UpdatePlanTool,
        ViewImageTool,
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
        assert_eq!(tokio::fs::read_to_string(&file).await.unwrap(), "hello\nyode\n");
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
        assert_eq!(tokio::fs::read_to_string(&file).await.unwrap(), "hello\nyode\n");
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
        assert_eq!(result.metadata.as_ref().unwrap()["mime_type"], json!("image/png"));
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
}
