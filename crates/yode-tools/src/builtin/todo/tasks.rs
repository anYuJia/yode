use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct TaskCreateTool;
pub struct TaskListTool;
pub struct TaskGetTool;

#[async_trait]
impl Tool for TaskCreateTool {
    fn name(&self) -> &str {
        "task_create"
    }

    fn aliases(&self) -> Vec<String> {
        vec!["TaskCreate".to_string()]
    }

    fn user_facing_name(&self) -> &str {
        ""
    }

    fn activity_description(&self, params: &Value) -> String {
        let subject = params
            .get("subject")
            .and_then(|v| v.as_str())
            .unwrap_or("task");
        format!("Creating task: {}", subject)
    }

    fn description(&self) -> &str {
        "Create a task in the task list. Use this to track long-running work or complex sub-tasks."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "subject": {
                    "type": "string",
                    "description": "A brief title for the task"
                },
                "description": {
                    "type": "string",
                    "description": "What needs to be done"
                }
            },
            "required": ["subject", "description"]
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
        let tasks = ctx
            .tasks
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Task store not available"))?;
        let subject = params
            .get("subject")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let description = params
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        let mut store = tasks.lock().await;
        let task = store.create(subject.to_string(), description.to_string());

        Ok(ToolResult::success(format!(
            "Task #{} created successfully: {}",
            task.id, task.subject
        )))
    }
}

#[async_trait]
impl Tool for TaskListTool {
    fn name(&self) -> &str {
        "task_list"
    }

    fn aliases(&self) -> Vec<String> {
        vec!["TaskList".to_string()]
    }

    fn user_facing_name(&self) -> &str {
        ""
    }

    fn activity_description(&self, _params: &Value) -> String {
        "Listing tasks".to_string()
    }

    fn description(&self) -> &str {
        "List all tasks in the session task list."
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
        let tasks = ctx
            .tasks
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Task store not available"))?;
        let store = tasks.lock().await;
        let all = store.list();

        if all.is_empty() {
            return Ok(ToolResult::success("No tasks found.".to_string()));
        }

        let mut output = String::from("Current tasks:\n\n");
        for task in all {
            let status = format!("{:?}", task.status);
            output.push_str(&format!(
                "- [#{}] {} ({})\n",
                task.id,
                task.subject,
                status.to_lowercase()
            ));
        }

        Ok(ToolResult::success(output))
    }
}

#[async_trait]
impl Tool for TaskGetTool {
    fn name(&self) -> &str {
        "task_get"
    }

    fn aliases(&self) -> Vec<String> {
        vec!["TaskGet".to_string()]
    }

    fn user_facing_name(&self) -> &str {
        ""
    }

    fn activity_description(&self, params: &Value) -> String {
        let id = params.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        format!("Getting task #{}", id)
    }

    fn description(&self) -> &str {
        "Get detailed information about a specific task by its ID."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "The ID of the task to retrieve"
                }
            },
            "required": ["id"]
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
        let tasks = ctx
            .tasks
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Task store not available"))?;
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing task ID"))?;

        let store = tasks.lock().await;
        match store.get(id) {
            Some(task) => {
                let status = format!("{:?}", task.status);
                let output = format!(
                    "Task #{} Details:\nSubject: {}\nStatus: {}\nDescription: {}",
                    task.id,
                    task.subject,
                    status.to_lowercase(),
                    task.description
                );
                Ok(ToolResult::success(output))
            }
            None => Ok(ToolResult::error(format!("Task #{} not found.", id))),
        }
    }
}
