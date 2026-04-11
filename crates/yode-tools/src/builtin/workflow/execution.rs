use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::WorkflowExecutionMode;
use crate::tool::{ToolCapabilities, ToolContext, ToolResult};

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

pub(super) fn workflow_parameters_schema() -> Value {
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

pub(super) async fn execute_workflow(
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
        .unwrap_or_else(|| PathBuf::from("."));
    let workflow_path = resolve_workflow_path(&params, &working_dir).ok_or_else(|| {
        ToolResult::error("Either 'name' or 'workflow_path' must be provided.".to_string())
    });

    let workflow_path = match workflow_path {
        Ok(path) => path,
        Err(result) => return Ok(result),
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
                "checkpoint": format!(
                    "Mutating step {} ({}) runs under workflow_run_with_writes confirmation.",
                    index + 1,
                    step.tool_name
                ),
            }));
        }

        let step_ctx = clone_step_context(ctx);
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

fn resolve_workflow_path(params: &Value, working_dir: &Path) -> Option<PathBuf> {
    params
        .get("workflow_path")
        .and_then(|value| value.as_str())
        .map(PathBuf::from)
        .or_else(|| {
            params
                .get("name")
                .and_then(|value| value.as_str())
                .map(|name| {
                    working_dir
                        .join(".yode")
                        .join("workflows")
                        .join(format!("{}.json", name))
                })
        })
}

fn clone_step_context(ctx: &ToolContext) -> ToolContext {
    ToolContext {
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
    }
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

pub(super) fn apply_variables(value: Value, variables: &Map<String, Value>) -> Value {
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
