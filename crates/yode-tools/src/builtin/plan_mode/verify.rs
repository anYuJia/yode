use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolErrorType, ToolResult};

pub struct VerifyPlanExecutionTool;

#[async_trait]
impl Tool for VerifyPlanExecutionTool {
    fn name(&self) -> &str {
        "verify_plan_execution"
    }

    fn aliases(&self) -> Vec<String> {
        vec!["VerifyPlanExecution".to_string()]
    }

    fn user_facing_name(&self) -> &str {
        ""
    }

    fn activity_description(&self, params: &Value) -> String {
        let status = params
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("verifying");
        format!("Verifying plan execution: {}", status)
    }

    fn description(&self) -> &str {
        "Verify that your plan has been executed successfully. This is the final step after completing the coding phase of a plan."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "summary": {
                    "type": "string",
                    "description": "A summary of what was verified and any remaining work"
                },
                "status": {
                    "type": "string",
                    "enum": ["success", "failed", "partial"],
                    "description": "Overall status of the plan implementation"
                }
            },
            "required": ["summary", "status"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: true,
        }
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let summary = params
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let status = params
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("success")
            .trim();

        if summary.is_empty() {
            return Ok(ToolResult::error_typed(
                "Missing required parameter: summary".to_string(),
                ToolErrorType::Validation,
                true,
                Some("Summarize what was verified and any remaining work.".to_string()),
            ));
        }
        if !matches!(status, "success" | "failed" | "partial") {
            return Ok(ToolResult::error_typed(
                format!(
                    "Invalid status: '{}'. Use success, failed, or partial.",
                    status
                ),
                ToolErrorType::Validation,
                true,
                Some("Set status to one of: success, failed, partial.".to_string()),
            ));
        }

        let output = format!(
            "Plan Verification (Status: {}):\n\n{}",
            status.to_uppercase(),
            summary
        );

        Ok(ToolResult::success(output))
    }
}
