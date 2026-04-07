use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct CronCreateTool;
pub struct CronListTool;
pub struct CronDeleteTool;

#[async_trait]
impl Tool for CronCreateTool {
    fn name(&self) -> &str {
        "cron_create"
    }

    fn aliases(&self) -> Vec<String> {
        vec!["CronCreate".to_string()]
    }

    fn user_facing_name(&self) -> &str {
        "" 
    }

    fn activity_description(&self, params: &Value) -> String {
        let cron = params.get("cron").and_then(|v| v.as_str()).unwrap_or("");
        format!("Scheduling cron job: {}", cron)
    }

    fn description(&self) -> &str {
        "Schedule a new cron job that will trigger a prompt on a specified schedule. \
         Jobs are session-scoped and auto-expire after 3 days. \
         Use standard 5-field cron syntax."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "cron": {
                    "type": "string",
                    "description": "5-field cron expression. E.g. '*/5 * * * *' for every 5 minutes."
                },
                "prompt": {
                    "type": "string",
                    "description": "The prompt to trigger when the cron fires."
                },
                "recurring": {
                    "type": "boolean",
                    "default": true,
                    "description": "Whether the job should fire repeatedly or just once."
                }
            },
            "required": ["cron", "prompt"]
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
        let cron_mgr = ctx.cron_manager.as_ref().ok_or_else(|| anyhow::anyhow!("Cron manager not available"))?;
        let cron_expr = params.get("cron").and_then(|v| v.as_str()).unwrap_or("");
        let prompt = params.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
        let recurring = params.get("recurring").and_then(|v| v.as_bool()).unwrap_or(true);

        let mut mgr = cron_mgr.lock().await;
        let id = mgr.create(cron_expr.to_string(), prompt.to_string(), recurring)?;
        Ok(ToolResult::success(format!("Cron job created with ID: {}. Note: recurring jobs expire after 3 days.", id)))
    }
}

#[async_trait]
impl Tool for CronListTool {
    fn name(&self) -> &str {
        "cron_list"
    }

    fn aliases(&self) -> Vec<String> {
        vec!["CronList".to_string()]
    }

    fn user_facing_name(&self) -> &str {
        ""
    }

    fn activity_description(&self, _params: &Value) -> String {
        "Listing scheduled cron jobs".to_string()
    }

    fn description(&self) -> &str {
        "List all currently scheduled cron jobs."
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
        let cron_mgr = ctx.cron_manager.as_ref().ok_or_else(|| anyhow::anyhow!("Cron manager not available"))?;
        let mgr = cron_mgr.lock().await;
        let jobs = mgr.list();
        
        if jobs.is_empty() {
            return Ok(ToolResult::success("No cron jobs scheduled.".to_string()));
        }

        let mut output = String::from("Current cron jobs:\n\n");
        for job in jobs {
            output.push_str(&format!("- ID: {}, cron: '{}', next_fire: {}\n", job.id, job.cron_expr, job.next_fire.format("%Y-%m-%d %H:%M:%S")));
        }
        Ok(ToolResult::success(output))
    }
}

#[async_trait]
impl Tool for CronDeleteTool {
    fn name(&self) -> &str {
        "cron_delete"
    }

    fn aliases(&self) -> Vec<String> {
        vec!["CronDelete".to_string()]
    }

    fn user_facing_name(&self) -> &str {
        ""
    }

    fn activity_description(&self, params: &Value) -> String {
        let id = params.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        format!("Deleting cron job: {}", id)
    }

    fn description(&self) -> &str {
        "Delete a scheduled cron job by its ID."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "The ID of the cron job to delete"
                }
            },
            "required": ["id"]
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
        let cron_mgr = ctx.cron_manager.as_ref().ok_or_else(|| anyhow::anyhow!("Cron manager not available"))?;
        let id = params.get("id").and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("Missing job ID"))?;
        
        let mut mgr = cron_mgr.lock().await;
        if mgr.delete(id) {
            Ok(ToolResult::success(format!("Cron job {} deleted.", id)))
        } else {
            Ok(ToolResult::error(format!("Cron job {} not found.", id)))
        }
    }
}
