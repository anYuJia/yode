use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub mod verify;

pub use verify::VerifyPlanExecutionTool;
pub struct EnterPlanModeTool;
pub struct ExitPlanModeTool;

#[async_trait]
impl Tool for EnterPlanModeTool {
    fn name(&self) -> &str {
        "enter_plan_mode"
    }

    fn user_facing_name(&self) -> &str {
        "" // Matches Claude's empty user facing name for this tool
    }

    fn activity_description(&self, _params: &Value) -> String {
        "Entering plan mode".to_string()
    }

    fn description(&self) -> &str {
        "Requests permission to enter plan mode for complex tasks requiring exploration and design."
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
        if let Some(plan_mode) = &ctx.plan_mode {
            let mut mode = plan_mode.lock().await;
            if *mode {
                return Ok(ToolResult::error("Already in plan mode.".to_string()));
            }
            *mode = true;
            
            let instructions = r#"Entered plan mode. You should now focus on exploring the codebase and designing an implementation approach.

In plan mode, you should:
1. Thoroughly explore the codebase to understand existing patterns
2. Identify similar features and architectural approaches
3. Consider multiple approaches and their trade-offs
4. Use ask_user if you need to clarify the approach
5. Design a concrete implementation strategy
6. When ready, use exit_plan_mode to present your plan for approval

Remember: DO NOT write or edit any files yet. This is a read-only exploration and planning phase."#;

            Ok(ToolResult::success(instructions.to_string()))
        } else {
            Ok(ToolResult::error("Plan mode is not supported in this context.".to_string()))
        }
    }
}

#[async_trait]
impl Tool for ExitPlanModeTool {
    fn name(&self) -> &str {
        "exit_plan_mode"
    }

    fn user_facing_name(&self) -> &str {
        ""
    }

    fn activity_description(&self, _params: &Value) -> String {
        "Exiting plan mode".to_string()
    }

    fn description(&self) -> &str {
        "Prompts the user to exit plan mode and start coding."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "allowedPrompts": {
                    "type": "array",
                    "description": "Prompt-based permissions needed to implement the plan. These describe categories of actions rather than specific commands.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "tool": {
                                "type": "string",
                                "enum": ["bash"],
                                "description": "The tool this prompt applies to"
                            },
                            "prompt": {
                                "type": "string",
                                "description": "Semantic description of the action, e.g. 'run tests', 'install dependencies'"
                            }
                        },
                        "required": ["tool", "prompt"]
                    }
                }
            }
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: true, // Exiting plan mode requires user approval
            supports_auto_execution: false,
            read_only: false,
        }
    }

    async fn execute(&self, _params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        if let Some(plan_mode) = &ctx.plan_mode {
            let mut mode = plan_mode.lock().await;
            if !*mode {
                return Ok(ToolResult::error("You are not in plan mode. This tool is only for exiting plan mode after writing a plan.".to_string()));
            }
            *mode = false;
            
            let output = r#"User has approved your plan. You can now start coding. Start with updating your todo list if applicable.

You can refer back to your plan if needed during implementation. Good luck!"#;

            Ok(ToolResult::success(output.to_string()))
        } else {
            Ok(ToolResult::error("Plan mode is not supported in this context.".to_string()))
        }
    }
}
