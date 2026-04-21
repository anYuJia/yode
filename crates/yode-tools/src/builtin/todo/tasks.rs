use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct TaskCreateTool;
pub struct TaskListTool;
pub struct TaskGetTool;
pub struct TaskUpdateTool;
pub struct TaskStopTool;

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

#[async_trait]
impl Tool for TaskUpdateTool {
    fn name(&self) -> &str {
        "task_update"
    }

    fn aliases(&self) -> Vec<String> {
        vec!["TaskUpdate".to_string()]
    }

    fn user_facing_name(&self) -> &str {
        ""
    }

    fn activity_description(&self, params: &Value) -> String {
        let id = params.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        format!("Updating task #{}", id)
    }

    fn description(&self) -> &str {
        "Update a session task's subject, description, or status."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "The ID of the task to update"
                },
                "subject": {
                    "type": "string",
                    "description": "Optional new subject"
                },
                "description": {
                    "type": "string",
                    "description": "Optional new description"
                },
                "status": {
                    "type": "string",
                    "enum": ["pending", "in_progress", "completed"],
                    "description": "Optional new status"
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
        let tasks = ctx
            .tasks
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Task store not available"))?;
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing task ID"))?;
        let subject = params
            .get("subject")
            .and_then(|v| v.as_str())
            .map(String::from);
        let description = params
            .get("description")
            .and_then(|v| v.as_str())
            .map(String::from);
        let status = params.get("status").and_then(|v| v.as_str()).map(|value| match value {
            "in_progress" => crate::state::TaskStatus::InProgress,
            "completed" => crate::state::TaskStatus::Completed,
            _ => crate::state::TaskStatus::Pending,
        });

        let mut store = tasks.lock().await;
        match store.update(id, subject, description, status) {
            Some(task) => Ok(ToolResult::success(format!(
                "Task #{} updated successfully: {}",
                task.id, task.subject
            ))),
            None => Ok(ToolResult::error(format!("Task #{} not found.", id))),
        }
    }
}

#[async_trait]
impl Tool for TaskStopTool {
    fn name(&self) -> &str {
        "task_stop"
    }

    fn aliases(&self) -> Vec<String> {
        vec!["TaskStop".to_string()]
    }

    fn user_facing_name(&self) -> &str {
        ""
    }

    fn activity_description(&self, params: &Value) -> String {
        let id = params
            .get("task_id")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        format!("Stopping runtime task {}", id)
    }

    fn description(&self) -> &str {
        "Request cancellation for a running background runtime task by its task ID."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The runtime task ID to stop"
                }
            },
            "required": ["task_id"]
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
        let runtime_tasks = ctx
            .runtime_tasks
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Runtime task store not available"))?;
        let task_id = params
            .get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing task_id"))?;

        let mut store = runtime_tasks.lock().await;
        if store.request_cancel(task_id) {
            Ok(ToolResult::success(format!(
                "Cancellation requested for runtime task '{}'.",
                task_id
            )))
        } else {
            Ok(ToolResult::error(format!(
                "Runtime task '{}' is not running or was not found.",
                task_id
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use tokio::sync::Mutex;

    use crate::runtime_tasks::RuntimeTaskStore;
    use crate::state::{TaskStatus, TaskStore};
    use crate::tool::{Tool, ToolContext};

    use super::{TaskCreateTool, TaskStopTool, TaskUpdateTool};

    #[tokio::test]
    async fn task_update_changes_subject_and_status() {
        let tasks = Arc::new(Mutex::new(TaskStore::new()));
        let created = {
            let mut store = tasks.lock().await;
            store.create("Old".to_string(), "Desc".to_string())
        };

        let mut ctx = ToolContext::empty();
        ctx.tasks = Some(tasks.clone());

        let result = TaskUpdateTool
            .execute(
                json!({
                    "id": created.id,
                    "subject": "New",
                    "status": "completed"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        let store = tasks.lock().await;
        let task = store.get("1").unwrap();
        assert_eq!(task.subject, "New");
        assert_eq!(task.status, TaskStatus::Completed);
    }

    #[tokio::test]
    async fn task_stop_requests_runtime_task_cancellation() {
        let runtime_tasks = Arc::new(Mutex::new(RuntimeTaskStore::new()));
        let mut store = runtime_tasks.lock().await;
        let (task, mut cancel_rx) = store.create(
            "bash".to_string(),
            "bash".to_string(),
            "run tests".to_string(),
            "/tmp/task.log".to_string(),
        );
        drop(store);

        let mut ctx = ToolContext::empty();
        ctx.runtime_tasks = Some(runtime_tasks.clone());

        let result = TaskStopTool
            .execute(
                json!({
                    "task_id": task.id
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        cancel_rx.changed().await.unwrap();
        assert!(*cancel_rx.borrow());
    }

    #[tokio::test]
    async fn task_create_still_works_with_new_task_tools_present() {
        let tasks = Arc::new(Mutex::new(TaskStore::new()));
        let mut ctx = ToolContext::empty();
        ctx.tasks = Some(tasks.clone());

        let result = TaskCreateTool
            .execute(
                json!({
                    "subject": "Task",
                    "description": "Do work"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        let store = tasks.lock().await;
        assert_eq!(store.list().len(), 1);
    }
}
