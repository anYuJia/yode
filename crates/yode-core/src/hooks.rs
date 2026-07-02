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
    SubagentStart,
    SubagentStop,
    TaskCreated,
    TaskCompleted,
    WorktreeCreate,
    PermissionRequest,
    PermissionDenied,
    UserPromptSubmit,
    ContextCompressed,
    Stop,
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
            Self::SubagentStart => write!(f, "subagent_start"),
            Self::SubagentStop => write!(f, "subagent_stop"),
            Self::TaskCreated => write!(f, "task_created"),
            Self::TaskCompleted => write!(f, "task_completed"),
            Self::WorktreeCreate => write!(f, "worktree_create"),
            Self::PermissionRequest => write!(f, "permission_request"),
            Self::PermissionDenied => write!(f, "permission_denied"),
            Self::UserPromptSubmit => write!(f, "user_prompt_submit"),
            Self::ContextCompressed => write!(f, "context_compressed"),
            Self::Stop => write!(f, "stop"),
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

impl HookContext {
    pub fn new(
        event: HookEvent,
        session_id: impl Into<String>,
        working_dir: impl Into<String>,
    ) -> Self {
        Self {
            event: event.to_string(),
            session_id: session_id.into(),
            working_dir: working_dir.into(),
            tool_name: None,
            tool_input: None,
            tool_output: None,
            error: None,
            user_prompt: None,
            metadata: None,
        }
    }

    pub fn with_tool(mut self, tool_name: impl Into<String>, input: Option<Value>) -> Self {
        self.tool_name = Some(tool_name.into());
        self.tool_input = input;
        self
    }

    pub fn with_tool_output(mut self, output: Option<String>) -> Self {
        self.tool_output = output;
        self
    }

    pub fn with_error(mut self, error: Option<String>) -> Self {
        self.error = error;
        self
    }

    pub fn with_user_prompt(mut self, user_prompt: Option<String>) -> Self {
        self.user_prompt = user_prompt;
        self
    }

    pub fn with_metadata(mut self, metadata: Option<Value>) -> Self {
        self.metadata = metadata;
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct HookResult {
    pub blocked: bool,
    pub deferred: bool,
    pub reason: Option<String>,
    pub modified_input: Option<Value>,
    pub stdout: Option<String>,
    pub wake_notification: Option<String>,
    pub source_hook_command: Option<String>,
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

    pub fn deferred(reason: String) -> Self {
        Self {
            deferred: true,
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
    pub defer_count: u32,
    pub last_failure_event: Option<String>,
    pub last_failure_command: Option<String>,
    pub last_failure_reason: Option<String>,
    pub last_failure_at: Option<String>,
    pub last_timeout_command: Option<String>,
    pub last_defer_command: Option<String>,
    pub last_defer_reason: Option<String>,
    pub last_defer_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookConfig {
    #[serde(default)]
    pub hooks: Vec<HookDefinition>,
}

#[derive(Debug, Clone, Default)]
pub struct PluginHookDiscovery {
    pub hooks: Vec<HookDefinition>,
    pub diagnostics: Vec<String>,
}

pub fn discover_plugin_hooks(project_root: &std::path::Path) -> PluginHookDiscovery {
    let mut discovery = PluginHookDiscovery::default();
    for path in crate::plugins::PluginRegistry::discover(project_root).enabled_hook_paths() {
        let hook_paths = expand_hook_contribution(path);
        for hook_path in hook_paths {
            match std::fs::read_to_string(&hook_path)
                .map_err(|err| format!("failed to read {}: {}", hook_path.display(), err))
                .and_then(|content| {
                    toml::from_str::<HookConfig>(&content).map_err(|err| {
                        format!("invalid hook manifest {}: {}", hook_path.display(), err)
                    })
                }) {
                Ok(config) => discovery.hooks.extend(config.hooks),
                Err(message) => discovery.diagnostics.push(message),
            }
        }
    }
    discovery
}

pub async fn discover_plugin_hooks_async(project_root: &std::path::Path) -> PluginHookDiscovery {
    let mut discovery = PluginHookDiscovery::default();
    let registry = crate::plugins::PluginRegistry::discover_async(project_root).await;
    for path in registry.enabled_hook_paths() {
        let hook_paths = expand_hook_contribution_async(path).await;
        for hook_path in hook_paths {
            match tokio::fs::read_to_string(&hook_path)
                .await
                .map_err(|err| format!("failed to read {}: {}", hook_path.display(), err))
                .and_then(|content| {
                    toml::from_str::<HookConfig>(&content).map_err(|err| {
                        format!("invalid hook manifest {}: {}", hook_path.display(), err)
                    })
                }) {
                Ok(config) => discovery.hooks.extend(config.hooks),
                Err(message) => discovery.diagnostics.push(message),
            }
        }
    }
    discovery
}

fn expand_hook_contribution(path: PathBuf) -> Vec<PathBuf> {
    if path.is_dir() {
        let mut paths = std::fs::read_dir(path)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("toml"))
            .collect::<Vec<_>>();
        paths.sort();
        return paths;
    }

    if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
        vec![path]
    } else {
        Vec::new()
    }
}

async fn expand_hook_contribution_async(path: PathBuf) -> Vec<PathBuf> {
    if tokio::fs::metadata(&path)
        .await
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        let mut entries = match tokio::fs::read_dir(path).await {
            Ok(entries) => entries,
            Err(_) => return Vec::new(),
        };
        let mut paths = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
                paths.push(path);
            }
        }
        paths.sort();
        return paths;
    }

    if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
        vec![path]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests;
