use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

#[derive(Debug, Deserialize)]
struct WorkflowFile {
    name: Option<String>,
    description: Option<String>,
    steps: Vec<WorkflowStep>,
}

#[derive(Debug, Deserialize)]
struct WorkflowStep {
    tool_name: String,
    #[serde(default)]
    params: Value,
    #[serde(default)]
    continue_on_error: bool,
}

pub struct WorkflowRunTool;
pub struct WorkflowRunWithWritesTool;

#[derive(Debug, Clone, Copy)]
enum WorkflowExecutionMode {
    SafeReadOnly,
    ConfirmedWrites,
}

#[async_trait]
impl Tool for WorkflowRunTool {
    fn name(&self) -> &str {
        "workflow_run"
    }

    fn user_facing_name(&self) -> &str {
        "Workflow"
    }

    fn activity_description(&self, params: &Value) -> String {
        let name = params
            .get("name")
            .and_then(|value| value.as_str())
            .or_else(|| params.get("workflow_path").and_then(|value| value.as_str()))
            .unwrap_or("workflow");
        format!("Running workflow: {}", name)
    }

    fn description(&self) -> &str {
        "Execute a predefined workflow script from .yode/workflows or an explicit JSON file path. This safe mode only allows read-only tools plus review/verification/coordinator helpers."
    }

    fn parameters_schema(&self) -> Value {
        workflow_parameters_schema()
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: true,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        execute_workflow(params, ctx, WorkflowExecutionMode::SafeReadOnly).await
    }
}

#[async_trait]
impl Tool for WorkflowRunWithWritesTool {
    fn name(&self) -> &str {
        "workflow_run_with_writes"
    }

    fn user_facing_name(&self) -> &str {
        "Workflow (write-enabled)"
    }

    fn activity_description(&self, params: &Value) -> String {
        let name = params
            .get("name")
            .and_then(|value| value.as_str())
            .or_else(|| params.get("workflow_path").and_then(|value| value.as_str()))
            .unwrap_or("workflow");
        format!("Running write-enabled workflow: {}", name)
    }

    fn description(&self) -> &str {
        "Execute a predefined workflow script that may call mutating tools. This tool requires user confirmation before the workflow starts; use workflow_run for safe read-only workflows."
    }

    fn parameters_schema(&self) -> Value {
        workflow_parameters_schema()
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: true,
            supports_auto_execution: false,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        execute_workflow(params, ctx, WorkflowExecutionMode::ConfirmedWrites).await
    }
}

fn workflow_parameters_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "Workflow name. Resolved to .yode/workflows/<name>.json in the current working directory."
            },
            "workflow_path": {
                "type": "string",
                "description": "Absolute path to a workflow JSON file."
            },
            "variables": {
                "type": "object",
                "description": "Optional ${var} substitutions applied recursively to workflow step params before execution."
            },
            "dry_run": {
                "type": "boolean",
                "default": false,
                "description": "If true, return the resolved workflow plan without executing any steps."
            }
        }
    })
}

async fn execute_workflow(
    params: Value,
    ctx: &ToolContext,
    mode: WorkflowExecutionMode,
) -> Result<ToolResult> {
    let registry = ctx
        .registry
        .as_ref()
        .map(Arc::clone)
        .ok_or_else(|| anyhow::anyhow!("Tool registry not available"))?;
    let working_dir = ctx
        .working_dir
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let workflow_path = if let Some(path) = params.get("workflow_path").and_then(|v| v.as_str()) {
        std::path::PathBuf::from(path)
    } else if let Some(name) = params.get("name").and_then(|v| v.as_str()) {
        working_dir
            .join(".yode")
            .join("workflows")
            .join(format!("{}.json", name))
    } else {
        return Ok(ToolResult::error(
            "Either 'name' or 'workflow_path' must be provided.".to_string(),
        ));
    };

    let content = tokio::fs::read_to_string(&workflow_path)
        .await
        .map_err(|err| {
            anyhow::anyhow!(
                "Failed to read workflow {}: {}",
                workflow_path.display(),
                err
            )
        })?;
    let workflow: WorkflowFile = serde_json::from_str(&content).map_err(|err| {
        anyhow::anyhow!(
            "Failed to parse workflow JSON {}: {}",
            workflow_path.display(),
            err
        )
    })?;
    let variables = params
        .get("variables")
        .and_then(|value| value.as_object())
        .cloned()
        .unwrap_or_default();
    let dry_run = params
        .get("dry_run")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

        if dry_run {
            let write_steps = workflow_write_checkpoints(&workflow.steps);
            let plan = workflow
                .steps
                .iter()
            .enumerate()
            .map(|(index, step)| {
                json!({
                    "index": index + 1,
                    "tool": step.tool_name,
                    "continue_on_error": step.continue_on_error,
                    "write_capable": is_write_capable_tool(&step.tool_name),
                    "params": apply_variables(step.params.clone(), &variables),
                })
            })
            .collect::<Vec<_>>();
        return Ok(ToolResult::success_with_metadata(
            serde_json::to_string_pretty(&plan)?,
            json!({
                "workflow_path": workflow_path,
                "workflow_name": workflow.name,
                "description": workflow.description,
                    "variables": variables,
                    "dry_run": true,
                    "mode": workflow_mode_label(mode),
                    "write_steps": write_steps,
                    "plan": plan,
                }),
            ));
        }

    let mut step_outputs = Vec::new();
    for (index, step) in workflow.steps.iter().enumerate() {
        let Some(tool) = registry.get(&step.tool_name) else {
            return Ok(ToolResult::error(format!(
                "Workflow step {} references unknown tool '{}'.",
                index + 1,
                step.tool_name
            )));
        };

        let caps = tool.capabilities();
        if is_workflow_tool(&step.tool_name) {
            return Ok(ToolResult::error(format!(
                    "Workflow step {} recursively invokes '{}', which is blocked to avoid nested workflow execution. Recovery: run `/workflows show <nested-name>` separately, or inline the nested workflow steps into this workflow after reviewing them with `/workflows preview`.",
                    index + 1,
                    step.tool_name
                )));
        }
        if matches!(mode, WorkflowExecutionMode::SafeReadOnly)
            && !is_safe_workflow_tool(&step.tool_name, caps)
        {
            return Ok(ToolResult::error(format!(
                    "Workflow step {} uses mutating tool '{}', which is blocked in safe workflow mode. Use workflow_run_with_writes if the user has explicitly approved this workflow.",
                    index + 1,
                    step.tool_name
                )));
        }

        let resolved_params = apply_variables(step.params.clone(), &variables);
        if matches!(mode, WorkflowExecutionMode::ConfirmedWrites)
            && is_write_capable_tool(&step.tool_name)
        {
            step_outputs.push(json!({
                "index": index + 1,
                "tool": step.tool_name,
                "approval_checkpoint": true,
                "checkpoint": format!("Mutating step {} ({}) runs under workflow_run_with_writes confirmation.", index + 1, step.tool_name),
            }));
        }
        let step_ctx = ToolContext {
            registry: ctx.registry.clone(),
            tasks: ctx.tasks.clone(),
            runtime_tasks: ctx.runtime_tasks.clone(),
            user_input_tx: ctx.user_input_tx.clone(),
            user_input_rx: ctx.user_input_rx.clone(),
            progress_tx: ctx.progress_tx.clone(),
            working_dir: ctx.working_dir.clone(),
            sub_agent_runner: ctx.sub_agent_runner.clone(),
            mcp_resources: ctx.mcp_resources.clone(),
            cron_manager: ctx.cron_manager.clone(),
            lsp_manager: ctx.lsp_manager.clone(),
            worktree_state: ctx.worktree_state.clone(),
            read_file_history: ctx.read_file_history.clone(),
            plan_mode: ctx.plan_mode.clone(),
        };

        let result = match tool.execute(resolved_params, &step_ctx).await {
            Ok(result) => result,
            Err(err) => ToolResult::error(format!("Step {} failed: {}", index + 1, err)),
        };
        let is_error = result.is_error;
        step_outputs.push(json!({
            "index": index + 1,
            "tool": step.tool_name,
            "is_error": is_error,
            "content": result.content,
        }));

        if is_error && !step.continue_on_error {
            break;
        }
    }

        Ok(ToolResult::success_with_metadata(
            serde_json::to_string_pretty(&step_outputs)?,
            json!({
                "workflow_path": workflow_path,
                "workflow_name": workflow.name,
                "description": workflow.description,
                "step_count": workflow.steps.len(),
                "variables": variables,
                "mode": workflow_mode_label(mode),
                "write_steps": workflow
                    .steps
                    .iter()
                    .enumerate()
                    .filter(|(_, step)| is_write_capable_tool(&step.tool_name))
                    .map(|(index, step)| {
                        json!({
                            "index": index + 1,
                            "tool": step.tool_name,
                        })
                    })
                    .collect::<Vec<_>>(),
                "approval_checkpoints": workflow_write_checkpoints(&workflow.steps),
                "results": step_outputs,
            }),
        ))
}

fn workflow_write_checkpoints(steps: &[WorkflowStep]) -> Vec<Value> {
    steps
        .iter()
        .enumerate()
        .filter(|(_, step)| is_write_capable_tool(&step.tool_name))
        .map(|(index, step)| {
            json!({
                "index": index + 1,
                "tool": step.tool_name,
                "requires": "workflow_run_with_writes confirmation",
            })
        })
        .collect()
}

fn workflow_mode_label(mode: WorkflowExecutionMode) -> &'static str {
    match mode {
        WorkflowExecutionMode::SafeReadOnly => "safe_read_only",
        WorkflowExecutionMode::ConfirmedWrites => "confirmed_writes",
    }
}

fn is_workflow_tool(tool_name: &str) -> bool {
    matches!(tool_name, "workflow_run" | "workflow_run_with_writes")
}

fn is_write_capable_tool(tool_name: &str) -> bool {
    !matches!(
        tool_name,
        "task_output"
            | "read_file"
            | "glob"
            | "grep"
            | "ls"
            | "git_status"
            | "git_diff"
            | "git_log"
            | "project_map"
            | "memory"
            | "review_changes"
            | "verification_agent"
            | "coordinate_agents"
    )
}

fn is_safe_workflow_tool(tool_name: &str, caps: ToolCapabilities) -> bool {
    caps.read_only || !is_write_capable_tool(tool_name)
}

fn apply_variables(value: Value, variables: &Map<String, Value>) -> Value {
    match value {
        Value::String(text) => Value::String(replace_variables(&text, variables)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| apply_variables(item, variables))
                .collect(),
        ),
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, value)| (key, apply_variables(value, variables)))
                .collect(),
        ),
        other => other,
    }
}

fn replace_variables(input: &str, variables: &Map<String, Value>) -> String {
    let mut output = input.to_string();
    for (key, value) in variables {
        let placeholder = format!("${{{}}}", key);
        let replacement = value
            .as_str()
            .map(|value| value.to_string())
            .unwrap_or_else(|| value.to_string());
        output = output.replace(&placeholder, &replacement);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{WorkflowRunTool, WorkflowRunWithWritesTool};
    use crate::registry::ToolRegistry;
    use crate::tool::{Tool, ToolContext};
    use std::sync::Arc;

    #[tokio::test]
    async fn workflow_runs_read_only_steps() {
        let dir = tempfile::tempdir().unwrap();
        let workflow_dir = dir.path().join(".yode").join("workflows");
        tokio::fs::create_dir_all(&workflow_dir).await.unwrap();
        tokio::fs::write(
            workflow_dir.join("inspect.json"),
            r#"{
                "name": "inspect",
                "steps": [
                    { "tool_name": "ls", "params": { "path": "." } }
                ]
            }"#,
        )
        .await
        .unwrap();

        let mut registry = ToolRegistry::new();
        crate::builtin::register_builtin_tools(&mut registry);

        let mut ctx = ToolContext::empty();
        ctx.registry = Some(Arc::new(registry));
        ctx.working_dir = Some(dir.path().to_path_buf());

        let tool = WorkflowRunTool;
        let result = tool
            .execute(serde_json::json!({ "name": "inspect" }), &ctx)
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("\"tool\": \"ls\""));
    }

    #[tokio::test]
    async fn workflow_dry_run_returns_plan_without_execution() {
        let dir = tempfile::tempdir().unwrap();
        let workflow_dir = dir.path().join(".yode").join("workflows");
        tokio::fs::create_dir_all(&workflow_dir).await.unwrap();
        tokio::fs::write(
            workflow_dir.join("plan.json"),
            r#"{
                "name": "plan",
                "steps": [
                    { "tool_name": "review_changes", "params": { "focus": "${focus}" } }
                ]
            }"#,
        )
        .await
        .unwrap();

        let mut registry = ToolRegistry::new();
        crate::builtin::register_builtin_tools(&mut registry);

        let mut ctx = ToolContext::empty();
        ctx.registry = Some(Arc::new(registry));
        ctx.working_dir = Some(dir.path().to_path_buf());

        let tool = WorkflowRunTool;
        let result = tool
            .execute(
                serde_json::json!({
                    "name": "plan",
                    "dry_run": true,
                    "variables": { "focus": "regressions" }
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("\"tool\": \"review_changes\""));
        assert!(result.content.contains("regressions"));
        assert!(result.metadata.unwrap()["write_steps"].is_array());
    }

    #[test]
    fn workflow_applies_variable_substitution() {
        let params = serde_json::json!({
            "command": "echo ${name}",
            "nested": ["${kind}"]
        });
        let variables = serde_json::json!({
            "name": "world",
            "kind": "read-only"
        })
        .as_object()
        .unwrap()
        .clone();
        let applied = super::apply_variables(params, &variables);
        assert_eq!(applied["command"].as_str(), Some("echo world"));
        assert_eq!(applied["nested"][0].as_str(), Some("read-only"));
    }

    #[tokio::test]
    async fn safe_workflow_blocks_mutating_tools() {
        let dir = tempfile::tempdir().unwrap();
        let output_path = dir.path().join("created.txt");
        let workflow_dir = dir.path().join(".yode").join("workflows");
        tokio::fs::create_dir_all(&workflow_dir).await.unwrap();
        tokio::fs::write(
            workflow_dir.join("write.json"),
            format!(
                r#"{{
                    "name": "write",
                    "steps": [
                        {{
                            "tool_name": "write_file",
                            "params": {{
                                "file_path": "{}",
                                "content": "hello"
                            }}
                        }}
                    ]
                }}"#,
                output_path.display()
            ),
        )
        .await
        .unwrap();

        let mut registry = ToolRegistry::new();
        crate::builtin::register_builtin_tools(&mut registry);

        let mut ctx = ToolContext::empty();
        ctx.registry = Some(Arc::new(registry));
        ctx.working_dir = Some(dir.path().to_path_buf());

        let tool = WorkflowRunTool;
        let result = tool
            .execute(serde_json::json!({ "name": "write" }), &ctx)
            .await
            .unwrap();

        assert!(result.is_error);
        assert!(result.content.contains("blocked in safe workflow mode"));
        assert!(!output_path.exists());
    }

    #[tokio::test]
    async fn write_enabled_workflow_can_run_mutating_steps() {
        let dir = tempfile::tempdir().unwrap();
        let output_path = dir.path().join("created.txt");
        let workflow_dir = dir.path().join(".yode").join("workflows");
        tokio::fs::create_dir_all(&workflow_dir).await.unwrap();
        tokio::fs::write(
            workflow_dir.join("write.json"),
            format!(
                r#"{{
                    "name": "write",
                    "steps": [
                        {{
                            "tool_name": "write_file",
                            "params": {{
                                "file_path": "{}",
                                "content": "hello"
                            }}
                        }}
                    ]
                }}"#,
                output_path.display()
            ),
        )
        .await
        .unwrap();

        let mut registry = ToolRegistry::new();
        crate::builtin::register_builtin_tools(&mut registry);

        let mut ctx = ToolContext::empty();
        ctx.registry = Some(Arc::new(registry));
        ctx.working_dir = Some(dir.path().to_path_buf());

        let tool = WorkflowRunWithWritesTool;
        let result = tool
            .execute(serde_json::json!({ "name": "write" }), &ctx)
            .await
            .unwrap();

        assert!(!result.is_error);
        assert_eq!(
            tokio::fs::read_to_string(&output_path).await.unwrap(),
            "hello"
        );
        let metadata = result.metadata.unwrap();
        assert_eq!(metadata["mode"], "confirmed_writes");
        assert!(metadata["approval_checkpoints"].is_array());
    }

    #[tokio::test]
    async fn workflow_blocks_recursive_execution() {
        let dir = tempfile::tempdir().unwrap();
        let workflow_dir = dir.path().join(".yode").join("workflows");
        tokio::fs::create_dir_all(&workflow_dir).await.unwrap();
        tokio::fs::write(
            workflow_dir.join("nested.json"),
            r#"{
                "name": "nested",
                "steps": [
                    { "tool_name": "workflow_run", "params": { "name": "other" } }
                ]
            }"#,
        )
        .await
        .unwrap();

        let mut registry = ToolRegistry::new();
        crate::builtin::register_builtin_tools(&mut registry);

        let mut ctx = ToolContext::empty();
        ctx.registry = Some(Arc::new(registry));
        ctx.working_dir = Some(dir.path().to_path_buf());

        let tool = WorkflowRunWithWritesTool;
        let result = tool
            .execute(serde_json::json!({ "name": "nested" }), &ctx)
            .await
            .unwrap();

        assert!(result.is_error);
        assert!(result
            .content
            .contains("blocked to avoid nested workflow execution"));
        assert!(result.content.contains("/workflows preview"));
    }
}
