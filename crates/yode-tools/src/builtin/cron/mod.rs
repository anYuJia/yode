use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct CronTool;

#[async_trait]
impl Tool for CronTool {
    fn name(&self) -> &str {
        "cron"
    }

    fn user_facing_name(&self) -> &str {
        "Cron"
    }

    fn activity_description(&self, params: &Value) -> String {
        let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("manage");
        format!("Cron: {} action", action)
    }

    fn description(&self) -> &str {
        "Schedule, list, or delete cron jobs that fire prompts on a schedule. \
         Jobs are session-scoped (not persisted) and auto-expire after 3 days. \
         Use standard 5-field cron expressions (minute hour day-of-month month day-of-week)."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create", "delete", "list"],
                    "description": "Action to perform"
                },
                "cron": {
                    "type": "string",
                    "description": "5-field cron expression (for create). E.g. '*/5 * * * *' = every 5 min"
                },
                "prompt": {
                    "type": "string",
                    "description": "Prompt to enqueue at each fire time (for create)"
                },
                "recurring": {
                    "type": "boolean",
                    "default": true,
                    "description": "true=fire repeatedly, false=fire once then delete"
                },
                "id": {
                    "type": "string",
                    "description": "Job ID to delete (for delete action)"
                }
            },
            "required": ["action"]
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
        let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("");

        let cron_mgr = ctx
            .cron_manager
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Cron manager not available"))?;

        match action {
            "create" => {
                let cron_expr = params
                    .get("cron")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("'cron' parameter is required for create"))?;
                let prompt = params
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("'prompt' parameter is required for create"))?;
                let recurring = params
                    .get("recurring")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);

                let mut mgr = cron_mgr.lock().await;
                match mgr.create(cron_expr.to_string(), prompt.to_string(), recurring) {
                    Ok(id) => Ok(ToolResult::success(format!(
                        "Cron job created with ID: {}. {}. Note: recurring jobs auto-expire after 3 days.",
                        id,
                        if recurring { "Fires repeatedly" } else { "Fires once" }
                    ))),
                    Err(e) => Ok(ToolResult::error(format!("Failed to create cron job: {}", e))),
                }
            }
            "delete" => {
                let id = params
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("'id' parameter is required for delete"))?;
                let mut mgr = cron_mgr.lock().await;
                if mgr.delete(id) {
                    Ok(ToolResult::success(format!("Cron job {} deleted", id)))
                } else {
                    Ok(ToolResult::error(format!("Cron job {} not found", id)))
                }
            }
            "list" => {
                let mgr = cron_mgr.lock().await;
                let jobs = mgr.list();
                if jobs.is_empty() {
                    Ok(ToolResult::success("No cron jobs scheduled.".to_string()))
                } else {
                    let mut output = String::new();
                    for job in jobs {
                        output.push_str(&format!(
                            "- ID: {}, cron: '{}', recurring: {}, next_fire: {}, prompt: '{}'\n",
                            job.id, job.cron_expr, job.recurring,
                            job.next_fire.format("%Y-%m-%d %H:%M:%S"),
                            if job.prompt.len() > 50 {
                                format!("{}...", &job.prompt[..50])
                            } else {
                                job.prompt.clone()
                            }
                        ));
                    }
                    Ok(ToolResult::success(output))
                }
            }
            _ => Ok(ToolResult::error(format!(
                "Unknown action: '{}'. Use create/delete/list.",
                action
            ))),
        }
    }
}
