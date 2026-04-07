use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::state::TaskStatus;
use crate::tool::{Tool, ToolContext, ToolResult};

pub struct TodoTool;

#[async_trait]
impl Tool for TodoTool {
    fn name(&self) -> &str {
        "todo"
    }

    fn user_facing_name(&self) -> &str {
        "Tasks"
    }

    fn activity_description(&self, params: &Value) -> String {
        let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("manage");
        format!("Task: {} action", action)
    }

    fn description(&self) -> &str {
        "Manage tasks for the current session. Supports create, update, list, get, and delete operations."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create", "update", "list", "get", "delete"],
                    "description": "The action to perform"
                },
                "id": {
                    "type": "string",
                    "description": "Task ID (required for update, get, delete)"
                },
                "subject": {
                    "type": "string",
                    "description": "Task subject/title (required for create, optional for update)"
                },
                "description": {
                    "type": "string",
                    "description": "Task description (optional)"
                },
                "status": {
                    "type": "string",
                    "enum": ["pending", "in_progress", "completed"],
                    "description": "Task status (for update)"
                }
            },
            "required": ["action"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        false
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let tasks = match &ctx.tasks {
            Some(t) => t,
            None => {
                return Ok(ToolResult::error(
                    "Task store not available.".to_string(),
                ));
            }
        };

        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: action"))?;

        match action {
            "create" => {
                let subject = params
                    .get("subject")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: subject"))?;
                let description = params
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let mut store = tasks.lock().await;
                let task = store.create(subject.to_string(), description.to_string());
                Ok(ToolResult::success(
                    serde_json::to_string_pretty(&task).unwrap(),
                ))
            }
            "update" => {
                let id = params
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: id"))?;

                let subject = params.get("subject").and_then(|v| v.as_str()).map(String::from);
                let description = params.get("description").and_then(|v| v.as_str()).map(String::from);
                let status = params
                    .get("status")
                    .and_then(|v| v.as_str())
                    .map(|s| match s {
                        "in_progress" => TaskStatus::InProgress,
                        "completed" => TaskStatus::Completed,
                        _ => TaskStatus::Pending,
                    });

                let mut store = tasks.lock().await;
                match store.update(id, subject, description, status) {
                    Some(task) => Ok(ToolResult::success(
                        serde_json::to_string_pretty(task).unwrap(),
                    )),
                    None => Ok(ToolResult::error(format!("Task '{}' not found.", id))),
                }
            }
            "list" => {
                let store = tasks.lock().await;
                let all = store.list();
                let output: Vec<Value> = all
                    .iter()
                    .map(|t| serde_json::to_value(t).unwrap())
                    .collect();
                Ok(ToolResult::success(
                    serde_json::to_string_pretty(&output).unwrap(),
                ))
            }
            "get" => {
                let id = params
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: id"))?;

                let store = tasks.lock().await;
                match store.get(id) {
                    Some(task) => Ok(ToolResult::success(
                        serde_json::to_string_pretty(task).unwrap(),
                    )),
                    None => Ok(ToolResult::error(format!("Task '{}' not found.", id))),
                }
            }
            "delete" => {
                let id = params
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: id"))?;

                let mut store = tasks.lock().await;
                if store.delete(id) {
                    Ok(ToolResult::success(format!("Task '{}' deleted.", id)))
                } else {
                    Ok(ToolResult::error(format!("Task '{}' not found.", id)))
                }
            }
            _ => Ok(ToolResult::error(format!(
                "Unknown action: '{}'. Use create, update, list, get, or delete.",
                action
            ))),
        }
    }
}
