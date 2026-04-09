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
        "Execute a predefined workflow script from .yode/workflows or an explicit JSON file path. This minimal version only allows read-only tools and task_output."
    }

    fn parameters_schema(&self) -> Value {
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
                }
            }
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
            working_dir.join(".yode").join("workflows").join(format!("{}.json", name))
        } else {
            return Ok(ToolResult::error(
                "Either 'name' or 'workflow_path' must be provided.".to_string(),
            ));
        };

        let content = tokio::fs::read_to_string(&workflow_path).await.map_err(|err| {
            anyhow::anyhow!("Failed to read workflow {}: {}", workflow_path.display(), err)
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
            let allowed = caps.read_only
                || matches!(
                    step.tool_name.as_str(),
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
                );
            if !allowed {
                return Ok(ToolResult::error(format!(
                    "Workflow step {} uses non-read-only tool '{}', which is blocked in safe workflow mode.",
                    index + 1,
                    step.tool_name
                )));
            }

            let resolved_params = apply_variables(step.params.clone(), &variables);
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
                "results": step_outputs,
            }),
        ))
    }
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
    use super::WorkflowRunTool;
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
}
