use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolErrorType, ToolResult};

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
            requires_confirmation: true,
            supports_auto_execution: false,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let cron_mgr = ctx
            .cron_manager
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Cron manager not available"))?;
        let cron_expr = params
            .get("cron")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let prompt = params
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if cron_expr.is_empty() {
            return Ok(ToolResult::error_typed(
                "Missing required parameter: cron".to_string(),
                ToolErrorType::Validation,
                true,
                Some("Provide a 5-field cron expression such as '*/5 * * * *'.".to_string()),
            ));
        }
        if prompt.is_empty() {
            return Ok(ToolResult::error_typed(
                "Missing required parameter: prompt".to_string(),
                ToolErrorType::Validation,
                true,
                Some("Provide the prompt to run when the cron job fires.".to_string()),
            ));
        }
        let recurring = params
            .get("recurring")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let mut mgr = cron_mgr.lock().await;
        let id = match mgr.create(cron_expr.to_string(), prompt.to_string(), recurring) {
            Ok(id) => id,
            Err(error) => {
                return Ok(ToolResult::error_typed(
                    error.to_string(),
                    ToolErrorType::Validation,
                    true,
                    Some("Use standard 5-field cron syntax, e.g. '*/5 * * * *'.".to_string()),
                ));
            }
        };
        Ok(ToolResult::success(format!(
            "Cron job created with ID: {}. Note: recurring jobs expire after 3 days.",
            id
        )))
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
        let cron_mgr = ctx
            .cron_manager
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Cron manager not available"))?;
        let mgr = cron_mgr.lock().await;
        let jobs = mgr.list();

        if jobs.is_empty() {
            return Ok(ToolResult::success("No cron jobs scheduled.".to_string()));
        }

        let mut output = String::from("Current cron jobs:\n\n");
        for job in jobs {
            output.push_str(&format!(
                "- ID: {}, cron: '{}', next_fire: {}\n",
                job.id,
                job.cron_expr,
                job.next_fire.format("%Y-%m-%d %H:%M:%S")
            ));
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
            requires_confirmation: true,
            supports_auto_execution: false,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let cron_mgr = ctx
            .cron_manager
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Cron manager not available"))?;
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing job ID"))?;

        let mut mgr = cron_mgr.lock().await;
        if mgr.delete(id) {
            Ok(ToolResult::success(format!("Cron job {} deleted.", id)))
        } else {
            Ok(ToolResult::error(format!("Cron job {} not found.", id)))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use tokio::sync::{mpsc, Mutex};

    use super::{CronCreateTool, CronDeleteTool, CronListTool};
    use crate::cron_manager::CronManager;
    use crate::tool::{Tool, ToolContext};

    fn ctx_with_cron_manager() -> ToolContext {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut ctx = ToolContext::empty();
        ctx.cron_manager = Some(Arc::new(Mutex::new(CronManager::new(tx))));
        ctx
    }

    #[tokio::test]
    async fn cron_tools_create_list_and_delete_job() {
        let ctx = ctx_with_cron_manager();
        let created = CronCreateTool
            .execute(
                json!({
                    "cron": "*/5 * * * *",
                    "prompt": "check status",
                    "recurring": true
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!created.is_error);
        assert!(created.content.contains("ID: 1"));

        let listed = CronListTool.execute(json!({}), &ctx).await.unwrap();
        assert!(!listed.is_error);
        assert!(listed.content.contains("cron: '*/5 * * * *'"));

        let deleted = CronDeleteTool
            .execute(json!({"id": "1"}), &ctx)
            .await
            .unwrap();
        assert!(!deleted.is_error);
        assert!(deleted.content.contains("deleted"));

        let listed = CronListTool.execute(json!({}), &ctx).await.unwrap();
        assert_eq!(listed.content, "No cron jobs scheduled.");
    }

    #[tokio::test]
    async fn cron_create_rejects_invalid_expression() {
        let ctx = ctx_with_cron_manager();
        let result = CronCreateTool
            .execute(
                json!({
                    "cron": "not a cron",
                    "prompt": "check status"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.is_error);
        assert_eq!(
            result.error_type,
            Some(crate::tool::ToolErrorType::Validation)
        );
        assert!(result.content.contains("Invalid cron expression"));
    }

    #[tokio::test]
    async fn cron_create_rejects_missing_prompt_as_validation_result() {
        let ctx = ctx_with_cron_manager();
        let result = CronCreateTool
            .execute(json!({"cron": "*/5 * * * *"}), &ctx)
            .await
            .unwrap();

        assert!(result.is_error);
        assert_eq!(
            result.error_type,
            Some(crate::tool::ToolErrorType::Validation)
        );
        assert!(result.content.contains("prompt"));
        assert!(result.recoverable);
    }

    #[test]
    fn cron_create_and_delete_require_confirmation() {
        let create_caps = CronCreateTool.capabilities();
        let delete_caps = CronDeleteTool.capabilities();
        let list_caps = CronListTool.capabilities();
        assert!(create_caps.requires_confirmation);
        assert!(!create_caps.supports_auto_execution);
        assert!(!create_caps.read_only);
        assert!(delete_caps.requires_confirmation);
        assert!(!delete_caps.supports_auto_execution);
        assert!(!delete_caps.read_only);
        assert!(!list_caps.requires_confirmation);
        assert!(list_caps.supports_auto_execution);
        assert!(list_caps.read_only);
    }
}
