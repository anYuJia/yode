use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Status of a task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
}

/// A single task in the task store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub subject: String,
    pub description: String,
    pub status: TaskStatus,
}

/// In-memory task store for the current session.
#[derive(Debug, Default)]
pub struct TaskStore {
    tasks: HashMap<String, Task>,
    next_id: u64,
}

impl TaskStore {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn create(&mut self, subject: String, description: String) -> Task {
        let id = self.next_id.to_string();
        self.next_id += 1;
        let task = Task {
            id: id.clone(),
            subject,
            description,
            status: TaskStatus::Pending,
        };
        self.tasks.insert(id, task.clone());
        task
    }

    pub fn get(&self, id: &str) -> Option<&Task> {
        self.tasks.get(id)
    }

    pub fn update_status(&mut self, id: &str, status: TaskStatus) -> Option<&Task> {
        if let Some(task) = self.tasks.get_mut(id) {
            task.status = status;
            Some(task)
        } else {
            None
        }
    }

    pub fn update(
        &mut self,
        id: &str,
        subject: Option<String>,
        description: Option<String>,
        status: Option<TaskStatus>,
    ) -> Option<&Task> {
        if let Some(task) = self.tasks.get_mut(id) {
            if let Some(s) = subject {
                task.subject = s;
            }
            if let Some(d) = description {
                task.description = d;
            }
            if let Some(st) = status {
                task.status = st;
            }
            Some(task)
        } else {
            None
        }
    }

    pub fn list(&self) -> Vec<&Task> {
        let mut tasks: Vec<&Task> = self.tasks.values().collect();
        tasks.sort_by(|a, b| a.id.cmp(&b.id));
        tasks
    }

    pub fn delete(&mut self, id: &str) -> bool {
        self.tasks.remove(id).is_some()
    }
}
