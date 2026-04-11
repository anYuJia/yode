mod manager;
mod parsing;

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    SessionStart,
    SessionEnd,
    PreTurn,
    PreCompact,
    PostCompact,
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
    PermissionRequest,
    PermissionDenied,
    UserPromptSubmit,
    ContextCompressed,
}

impl std::fmt::Display for HookEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionStart => write!(f, "session_start"),
            Self::SessionEnd => write!(f, "session_end"),
            Self::PreTurn => write!(f, "pre_turn"),
            Self::PreCompact => write!(f, "pre_compact"),
            Self::PostCompact => write!(f, "post_compact"),
            Self::PreToolUse => write!(f, "pre_tool_use"),
            Self::PostToolUse => write!(f, "post_tool_use"),
            Self::PostToolUseFailure => write!(f, "post_tool_use_failure"),
            Self::PermissionRequest => write!(f, "permission_request"),
            Self::PermissionDenied => write!(f, "permission_denied"),
            Self::UserPromptSubmit => write!(f, "user_prompt_submit"),
            Self::ContextCompressed => write!(f, "context_compressed"),
        }
    }
}

/// Data passed to hook handlers.
#[derive(Debug, Clone, Serialize)]
pub struct HookContext {
    pub event: String,
    pub session_id: String,
    pub working_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Default)]
pub struct HookResult {
    pub blocked: bool,
    pub reason: Option<String>,
    pub modified_input: Option<Value>,
    pub stdout: Option<String>,
    pub wake_notification: Option<String>,
}

impl HookResult {
    pub fn allowed() -> Self {
        Self::default()
    }

    pub fn blocked(reason: String) -> Self {
        Self {
            blocked: true,
            reason: Some(reason),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDefinition {
    pub command: String,
    pub events: Vec<String>,
    #[serde(default)]
    pub tool_filter: Option<Vec<String>>,
    #[serde(default = "default_hook_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub can_block: bool,
}

fn default_hook_timeout() -> u64 {
    10
}

pub struct HookManager {
    hooks: Vec<HookDefinition>,
    working_dir: PathBuf,
    wake_notifications: Mutex<Vec<WakeNotification>>,
    stats: Mutex<HookManagerStats>,
}

#[derive(Debug, Clone)]
pub struct WakeNotification {
    pub event: String,
    pub hook_command: String,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct HookManagerStats {
    pub total_executions: u32,
    pub timeout_count: u32,
    pub execution_error_count: u32,
    pub nonzero_exit_count: u32,
    pub wake_notification_count: u32,
    pub last_failure_event: Option<String>,
    pub last_failure_command: Option<String>,
    pub last_failure_reason: Option<String>,
    pub last_failure_at: Option<String>,
    pub last_timeout_command: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookConfig {
    #[serde(default)]
    pub hooks: Vec<HookDefinition>,
}

#[cfg(test)]
mod tests;
