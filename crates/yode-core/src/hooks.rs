use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ─── Hook Events ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    // Session lifecycle
    SessionStart,
    SessionEnd,
    // Turn lifecycle
    PreTurn,
    // Tool execution
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
    // Permission
    PermissionRequest,
    PermissionDenied,
    // User interaction
    UserPromptSubmit,
    // Context
    ContextCompressed,
}

impl std::fmt::Display for HookEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionStart => write!(f, "session_start"),
            Self::SessionEnd => write!(f, "session_end"),
            Self::PreTurn => write!(f, "pre_turn"),
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

// ─── Hook Context ───────────────────────────────────────────────────────────

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
}

// ─── Hook Result ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct HookResult {
    /// If true, the operation should be blocked/cancelled.
    pub blocked: bool,
    /// Reason for blocking.
    pub reason: Option<String>,
    /// Modified input (for pre_tool_use hooks that want to transform input).
    pub modified_input: Option<Value>,
    /// Stdout from the hook command.
    pub stdout: Option<String>,
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

// ─── Hook Definition ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDefinition {
    /// Shell command to execute.
    pub command: String,
    /// Events this hook listens to.
    pub events: Vec<String>,
    /// Optional: only trigger for specific tool names.
    #[serde(default)]
    pub tool_filter: Option<Vec<String>>,
    /// Timeout in seconds (default: 10).
    #[serde(default = "default_hook_timeout")]
    pub timeout_secs: u64,
    /// Whether blocking result should prevent the operation.
    #[serde(default)]
    pub can_block: bool,
}

fn default_hook_timeout() -> u64 {
    10
}

// ─── Hook Manager ───────────────────────────────────────────────────────────

pub struct HookManager {
    hooks: Vec<HookDefinition>,
    working_dir: PathBuf,
}

impl HookManager {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            hooks: Vec::new(),
            working_dir,
        }
    }

    pub fn register(&mut self, hook: HookDefinition) {
        self.hooks.push(hook);
    }

    pub fn register_all(&mut self, hooks: Vec<HookDefinition>) {
        self.hooks.extend(hooks);
    }

    /// Execute all hooks matching the given event.
    pub async fn execute(&self, event: HookEvent, context: &HookContext) -> Vec<HookResult> {
        let event_str = event.to_string();
        let matching: Vec<&HookDefinition> = self
            .hooks
            .iter()
            .filter(|h| h.events.iter().any(|e| e == &event_str))
            .filter(|h| {
                // Apply tool filter if present
                if let Some(ref filter) = h.tool_filter {
                    if let Some(ref tool_name) = context.tool_name {
                        filter.iter().any(|f| f == tool_name)
                    } else {
                        true
                    }
                } else {
                    true
                }
            })
            .collect();

        let mut results = Vec::new();

        for hook in matching {
            let result = self.execute_hook(hook, context).await;
            results.push(result);
        }

        results
    }

    /// Check if any blocking hook prevents an operation.
    pub async fn check_blocked(
        &self,
        event: HookEvent,
        context: &HookContext,
    ) -> Option<HookResult> {
        let results = self.execute(event, context).await;
        results.into_iter().find(|r| r.blocked)
    }

    async fn execute_hook(&self, hook: &HookDefinition, context: &HookContext) -> HookResult {
        let context_json = match serde_json::to_string(context) {
            Ok(j) => j,
            Err(e) => {
                tracing::warn!("Failed to serialize hook context: {}", e);
                return HookResult::allowed();
            }
        };

        let timeout = std::time::Duration::from_secs(hook.timeout_secs);

        let result = tokio::time::timeout(timeout, async {
            tokio::process::Command::new("sh")
                .arg("-c")
                .arg(&hook.command)
                .env("YODE_HOOK_CONTEXT", &context_json)
                .env("YODE_HOOK_EVENT", &context.event)
                .current_dir(&self.working_dir)
                .output()
                .await
        })
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                if !output.status.success() && hook.can_block {
                    HookResult {
                        blocked: true,
                        reason: Some(if stderr.is_empty() {
                            format!("Hook '{}' exited with code {}", hook.command, output.status)
                        } else {
                            stderr.trim().to_string()
                        }),
                        modified_input: None,
                        stdout: Some(stdout),
                    }
                } else {
                    HookResult {
                        blocked: false,
                        reason: None,
                        modified_input: None,
                        stdout: if stdout.is_empty() {
                            None
                        } else {
                            Some(stdout)
                        },
                    }
                }
            }
            Ok(Err(e)) => {
                tracing::warn!("Hook execution failed: {}", e);
                HookResult::allowed()
            }
            Err(_) => {
                tracing::warn!(
                    "Hook '{}' timed out after {}s",
                    hook.command,
                    hook.timeout_secs
                );
                HookResult::allowed()
            }
        }
    }
}

// ─── Hook Config (for TOML) ────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookConfig {
    #[serde(default)]
    pub hooks: Vec<HookDefinition>,
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_event_display() {
        assert_eq!(HookEvent::PreToolUse.to_string(), "pre_tool_use");
        assert_eq!(HookEvent::SessionStart.to_string(), "session_start");
    }

    #[test]
    fn test_hook_result_default() {
        let r = HookResult::allowed();
        assert!(!r.blocked);
        assert!(r.reason.is_none());
    }

    #[test]
    fn test_hook_result_blocked() {
        let r = HookResult::blocked("denied".into());
        assert!(r.blocked);
        assert_eq!(r.reason.as_deref(), Some("denied"));
    }

    #[tokio::test]
    async fn test_hook_manager_no_hooks() {
        let mgr = HookManager::new(PathBuf::from("/tmp"));
        let ctx = HookContext {
            event: "pre_tool_use".into(),
            session_id: "test".into(),
            working_dir: "/tmp".into(),
            tool_name: Some("bash".into()),
            tool_input: None,
            tool_output: None,
            error: None,
            user_prompt: None,
        };
        let results = mgr.execute(HookEvent::PreToolUse, &ctx).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_hook_manager_with_echo_hook() {
        let mut mgr = HookManager::new(PathBuf::from("/tmp"));
        mgr.register(HookDefinition {
            command: "echo hello".into(),
            events: vec!["pre_tool_use".into()],
            tool_filter: None,
            timeout_secs: 5,
            can_block: false,
        });
        let ctx = HookContext {
            event: "pre_tool_use".into(),
            session_id: "test".into(),
            working_dir: "/tmp".into(),
            tool_name: Some("bash".into()),
            tool_input: None,
            tool_output: None,
            error: None,
            user_prompt: None,
        };
        let results = mgr.execute(HookEvent::PreToolUse, &ctx).await;
        assert_eq!(results.len(), 1);
        assert!(!results[0].blocked);
        assert_eq!(results[0].stdout.as_deref(), Some("hello\n"));
    }

    #[tokio::test]
    async fn test_hook_tool_filter() {
        let mut mgr = HookManager::new(PathBuf::from("/tmp"));
        mgr.register(HookDefinition {
            command: "echo filtered".into(),
            events: vec!["pre_tool_use".into()],
            tool_filter: Some(vec!["write_file".into()]),
            timeout_secs: 5,
            can_block: false,
        });
        // Should not match "bash"
        let ctx = HookContext {
            event: "pre_tool_use".into(),
            session_id: "test".into(),
            working_dir: "/tmp".into(),
            tool_name: Some("bash".into()),
            tool_input: None,
            tool_output: None,
            error: None,
            user_prompt: None,
        };
        let results = mgr.execute(HookEvent::PreToolUse, &ctx).await;
        assert!(results.is_empty());
    }
}
