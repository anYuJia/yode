use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tokio::sync::watch;

const MAX_PROGRESS_HISTORY: usize = 8;
const DEFAULT_MAX_COMPLETED_TASKS: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeTask {
    pub id: String,
    pub kind: String,
    pub source_tool: String,
    pub description: String,
    pub status: RuntimeTaskStatus,
    pub attempt: u32,
    pub retry_of: Option<String>,
    pub output_path: String,
    #[serde(default)]
    pub transcript_path: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub last_progress: Option<String>,
    pub last_progress_at: Option<String>,
    pub progress_history: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeTaskNotification {
    pub task_id: String,
    pub severity: RuntimeTaskNotificationSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTaskNotificationSeverity {
    Info,
    Success,
    Warning,
    Error,
}

impl RuntimeTaskNotificationSeverity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Default)]
pub struct RuntimeTaskStore {
    tasks: HashMap<String, RuntimeTask>,
    controls: HashMap<String, watch::Sender<bool>>,
    notifications: Vec<RuntimeTaskNotification>,
    next_id: u64,
}

impl RuntimeTaskStore {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            controls: HashMap::new(),
            notifications: Vec::new(),
            next_id: 1,
        }
    }

    pub fn create(
        &mut self,
        kind: String,
        source_tool: String,
        description: String,
        output_path: String,
    ) -> (RuntimeTask, watch::Receiver<bool>) {
        self.create_with_transcript(kind, source_tool, description, output_path, None)
    }

    pub fn create_with_transcript(
        &mut self,
        kind: String,
        source_tool: String,
        description: String,
        output_path: String,
        transcript_path: Option<String>,
    ) -> (RuntimeTask, watch::Receiver<bool>) {
        let retry_parent = self
            .tasks
            .values()
            .filter(|task| {
                task.kind == kind
                    && task.source_tool == source_tool
                    && task.description == description
                    && matches!(
                        task.status,
                        RuntimeTaskStatus::Failed | RuntimeTaskStatus::Cancelled
                    )
            })
            .max_by_key(|task| task.id.clone())
            .cloned();
        let id = format!("task-{}", self.next_id);
        self.next_id += 1;
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let task = RuntimeTask {
            id: id.clone(),
            kind,
            source_tool,
            description,
            status: RuntimeTaskStatus::Pending,
            attempt: retry_parent
                .as_ref()
                .map(|task| task.attempt.saturating_add(1))
                .unwrap_or(1),
            retry_of: retry_parent.as_ref().map(|task| task.id.clone()),
            output_path,
            transcript_path,
            created_at: now_string(),
            started_at: None,
            completed_at: None,
            last_progress: None,
            last_progress_at: None,
            progress_history: Vec::new(),
            error: None,
        };
        self.tasks.insert(id.clone(), task.clone());
        self.controls.insert(id, cancel_tx);
        (task, cancel_rx)
    }

    pub fn list(&self) -> Vec<RuntimeTask> {
        let mut tasks = self.tasks.values().cloned().collect::<Vec<_>>();
        tasks.sort_by(|a, b| a.id.cmp(&b.id));
        tasks
    }

    pub fn get(&self, id: &str) -> Option<RuntimeTask> {
        self.tasks.get(id).cloned()
    }

    pub fn mark_running(&mut self, id: &str) {
        if let Some(task) = self.tasks.get_mut(id) {
            task.status = RuntimeTaskStatus::Running;
            if task.started_at.is_none() {
                task.started_at = Some(now_string());
            }
        }
    }

    pub fn update_progress(&mut self, id: &str, message: String) {
        if let Some(task) = self.tasks.get_mut(id) {
            if message.trim().is_empty() {
                return;
            }
            task.last_progress = Some(message.clone());
            task.last_progress_at = Some(now_string());
            if task.progress_history.last() != Some(&message) {
                task.progress_history.push(message);
                if task.progress_history.len() > MAX_PROGRESS_HISTORY {
                    let extra = task.progress_history.len() - MAX_PROGRESS_HISTORY;
                    task.progress_history.drain(0..extra);
                }
            }
        }
    }

    pub fn mark_completed(&mut self, id: &str) {
        if let Some(task) = self.tasks.get_mut(id) {
            task.status = RuntimeTaskStatus::Completed;
            task.completed_at = Some(now_string());
            self.notifications.push(RuntimeTaskNotification {
                task_id: id.to_string(),
                severity: RuntimeTaskNotificationSeverity::Success,
                message: format!("Task {} completed: {}", id, task.description),
            });
        }
        self.controls.remove(id);
        self.prune_completed();
    }

    pub fn mark_failed(&mut self, id: &str, error: String) {
        if let Some(task) = self.tasks.get_mut(id) {
            task.status = RuntimeTaskStatus::Failed;
            task.completed_at = Some(now_string());
            task.error = Some(error.clone());
            self.notifications.push(RuntimeTaskNotification {
                task_id: id.to_string(),
                severity: RuntimeTaskNotificationSeverity::Error,
                message: format!("Task {} failed: {}", id, error),
            });
        }
        self.controls.remove(id);
        self.prune_completed();
    }

    pub fn mark_cancelled(&mut self, id: &str) {
        if let Some(task) = self.tasks.get_mut(id) {
            task.status = RuntimeTaskStatus::Cancelled;
            task.completed_at = Some(now_string());
            self.notifications.push(RuntimeTaskNotification {
                task_id: id.to_string(),
                severity: RuntimeTaskNotificationSeverity::Warning,
                message: format!("Task {} cancelled.", id),
            });
        }
        self.controls.remove(id);
        self.prune_completed();
    }

    pub fn request_cancel(&mut self, id: &str) -> bool {
        if let Some(control) = self.controls.get(id) {
            let _ = control.send(true);
            true
        } else {
            false
        }
    }

    pub fn drain_notifications(&mut self) -> Vec<RuntimeTaskNotification> {
        std::mem::take(&mut self.notifications)
    }

    fn prune_completed(&mut self) {
        let max_completed_tasks = max_completed_task_retention();

        let mut finished = self
            .tasks
            .values()
            .filter(|task| {
                matches!(
                    task.status,
                    RuntimeTaskStatus::Completed
                        | RuntimeTaskStatus::Failed
                        | RuntimeTaskStatus::Cancelled
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        if finished.len() <= max_completed_tasks {
            return;
        }

        finished.sort_by(|a, b| a.completed_at.cmp(&b.completed_at));
        let remove_count = finished.len().saturating_sub(max_completed_tasks);
        for task in finished.into_iter().take(remove_count) {
            if !task.output_path.is_empty() {
                let _ = std::fs::remove_file(&task.output_path);
            }
            self.tasks.remove(&task.id);
            self.controls.remove(&task.id);
        }
    }
}

fn now_string() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn max_completed_task_retention() -> usize {
    retention_from_env(
        std::env::var("YODE_MAX_COMPLETED_RUNTIME_TASKS")
            .ok()
            .as_deref(),
    )
}

pub(super) fn retention_from_env(value: Option<&str>) -> usize {
    value
        .and_then(|raw| raw.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_COMPLETED_TASKS)
}
