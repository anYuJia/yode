use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct EnterPlanModeTool;
pub struct ExitPlanModeTool;

#[async_trait]
impl Tool for EnterPlanModeTool {
    fn name(&self) -> &str {
        "enter_plan_mode"
    }

    fn description(&self) -> &str {
        "Enter plan mode to design an implementation approach before writing code. \
         In plan mode, only read-only tools are available. Use this proactively when \
         about to start a non-trivial implementation task."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
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
        if let Some(plan_mode) = &ctx.plan_mode {
            if plan_mode.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(ToolResult::error("Already in plan mode.".to_string()));
            }
            plan_mode.store(true, std::sync::atomic::Ordering::Relaxed);
            Ok(ToolResult::success(
                "Entered plan mode. Only read-only tools are available. \
                 Use exit_plan_mode when your plan is ready for approval."
                    .to_string(),
            ))
        } else {
            Ok(ToolResult::error(
                "Plan mode is not supported in this context.".to_string(),
            ))
        }
    }
}

#[async_trait]
impl Tool for ExitPlanModeTool {
    fn name(&self) -> &str {
        "exit_plan_mode"
    }

    fn description(&self) -> &str {
        "Exit plan mode and signal that the plan is ready for user approval. \
         The plan should have been written to a plan file before calling this."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
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
        if let Some(plan_mode) = &ctx.plan_mode {
            if !plan_mode.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(ToolResult::error("Not in plan mode.".to_string()));
            }
            plan_mode.store(false, std::sync::atomic::Ordering::Relaxed);
            Ok(ToolResult::success(
                "Exited plan mode. Ready for implementation.".to_string(),
            ))
        } else {
            Ok(ToolResult::error(
                "Plan mode is not supported in this context.".to_string(),
            ))
        }
    }
}
