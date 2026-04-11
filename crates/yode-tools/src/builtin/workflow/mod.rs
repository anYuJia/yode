mod execution;
#[cfg(test)]
mod tests;
mod variables;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

use self::execution::{execute_workflow, workflow_parameters_schema};

pub struct WorkflowRunTool;
pub struct WorkflowRunWithWritesTool;

#[derive(Debug, Clone, Copy)]
pub(super) enum WorkflowExecutionMode {
    SafeReadOnly,
    ConfirmedWrites,
}

fn workflow_name(params: &Value) -> &str {
    params
        .get("name")
        .and_then(|value| value.as_str())
        .or_else(|| params.get("workflow_path").and_then(|value| value.as_str()))
        .unwrap_or("workflow")
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
        format!("Running workflow: {}", workflow_name(params))
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
        format!("Running write-enabled workflow: {}", workflow_name(params))
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
