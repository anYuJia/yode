use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

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
    // Context compaction
    PreCompact,
    PostCompact,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
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
    /// Async wake notification requested by the hook.
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

impl HookManager {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            hooks: Vec::new(),
            working_dir,
            wake_notifications: Mutex::new(Vec::new()),
            stats: Mutex::new(HookManagerStats::default()),
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
            if let Some(message) = result.wake_notification.clone() {
                if let Ok(mut notifications) = self.wake_notifications.lock() {
                    notifications.push(WakeNotification {
                        event: context.event.clone(),
                        hook_command: hook.command.clone(),
                        message,
                    });
                }
            }
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

    pub fn drain_wake_notifications(&self) -> Vec<WakeNotification> {
        if let Ok(mut notifications) = self.wake_notifications.lock() {
            std::mem::take(&mut *notifications)
        } else {
            Vec::new()
        }
    }

    pub fn stats_snapshot(&self) -> HookManagerStats {
        self.stats
            .lock()
            .map(|stats| stats.clone())
            .unwrap_or_default()
    }

    async fn execute_hook(&self, hook: &HookDefinition, context: &HookContext) -> HookResult {
        self.record_hook_attempt();
        let context_json = match serde_json::to_string(context) {
            Ok(j) => j,
            Err(e) => {
                tracing::warn!("Failed to serialize hook context: {}", e);
                return HookResult::allowed();
            }
        };

        let timeout = std::time::Duration::from_secs(hook.timeout_secs);
        let started_at = Instant::now();

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
                let mut structured = parse_structured_hook_output(&stdout);
                if let Some(ref mut parsed) = structured {
                    if parsed.blocked && !hook.can_block {
                        parsed.blocked = false;
                    }
                }

                if output.status.code() == Some(2) {
                    self.record_hook_wake();
                    let wake_message = structured
                        .as_ref()
                        .and_then(|parsed| parsed.wake_notification.clone())
                        .or_else(|| {
                            let trimmed = stdout.trim();
                            if !trimmed.is_empty() {
                                Some(trimmed.to_string())
                            } else {
                                let trimmed = stderr.trim();
                                if !trimmed.is_empty() {
                                    Some(trimmed.to_string())
                                } else {
                                    Some(format!(
                                        "Hook '{}' requested wake notification",
                                        hook.command
                                    ))
                                }
                            }
                        });

                    if let Some(mut parsed) = structured {
                        parsed.blocked = false;
                        parsed.wake_notification = wake_message;
                        return parsed;
                    }

                    return HookResult {
                        blocked: false,
                        reason: None,
                        modified_input: None,
                        stdout: if stdout.is_empty() {
                            None
                        } else {
                            Some(stdout)
                        },
                        wake_notification: wake_message,
                    };
                }

                if !output.status.success() && hook.can_block {
                    self.record_hook_failure(
                        &context.event,
                        &hook.command,
                        if stderr.is_empty() {
                            format!(
                                "non-zero exit after {}ms: {}",
                                started_at.elapsed().as_millis(),
                                output.status
                            )
                        } else {
                            format!(
                                "non-zero exit after {}ms: {}",
                                started_at.elapsed().as_millis(),
                                stderr.trim()
                            )
                        },
                        true,
                    );
                    if let Some(mut parsed) = structured {
                        if parsed.reason.is_none() {
                            parsed.reason = Some(if stderr.is_empty() {
                                format!(
                                    "Hook '{}' exited with code {}",
                                    hook.command, output.status
                                )
                            } else {
                                stderr.trim().to_string()
                            });
                        }
                        parsed.blocked = true;
                        parsed
                    } else {
                        HookResult {
                            blocked: true,
                            reason: Some(if stderr.is_empty() {
                                format!(
                                    "Hook '{}' exited with code {}",
                                    hook.command, output.status
                                )
                            } else {
                                stderr.trim().to_string()
                            }),
                            modified_input: None,
                            stdout: Some(stdout),
                            wake_notification: None,
                        }
                    }
                } else if let Some(parsed) = structured {
                    parsed
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
                        wake_notification: None,
                    }
                }
            }
            Ok(Err(e)) => {
                tracing::warn!("Hook execution failed: {}", e);
                self.record_hook_failure(
                    &context.event,
                    &hook.command,
                    format!(
                        "spawn/exec error after {}ms: {}",
                        started_at.elapsed().as_millis(),
                        e
                    ),
                    false,
                );
                HookResult::allowed()
            }
            Err(_) => {
                tracing::warn!(
                    "Hook '{}' timed out after {}s (event={})",
                    hook.command,
                    hook.timeout_secs,
                    context.event,
                );
                self.record_hook_timeout(&context.event, &hook.command, hook.timeout_secs);
                HookResult::allowed()
            }
        }
    }

    fn record_hook_attempt(&self) {
        if let Ok(mut stats) = self.stats.lock() {
            stats.total_executions = stats.total_executions.saturating_add(1);
        }
    }

    fn record_hook_wake(&self) {
        if let Ok(mut stats) = self.stats.lock() {
            stats.wake_notification_count = stats.wake_notification_count.saturating_add(1);
        }
    }

    fn record_hook_timeout(&self, event: &str, command: &str, timeout_secs: u64) {
        if let Ok(mut stats) = self.stats.lock() {
            stats.timeout_count = stats.timeout_count.saturating_add(1);
            stats.last_failure_event = Some(event.to_string());
            stats.last_failure_command = Some(command.to_string());
            stats.last_failure_reason = Some(format!("timed out after {}s", timeout_secs));
            stats.last_failure_at =
                Some(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
            stats.last_timeout_command = Some(command.to_string());
        }
    }

    fn record_hook_failure(&self, event: &str, command: &str, reason: String, nonzero_exit: bool) {
        if let Ok(mut stats) = self.stats.lock() {
            if nonzero_exit {
                stats.nonzero_exit_count = stats.nonzero_exit_count.saturating_add(1);
            } else {
                stats.execution_error_count = stats.execution_error_count.saturating_add(1);
            }
            stats.last_failure_event = Some(event.to_string());
            stats.last_failure_command = Some(command.to_string());
            stats.last_failure_reason = Some(reason);
            stats.last_failure_at =
                Some(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
        }
    }
}

fn parse_structured_hook_output(stdout: &str) -> Option<HookResult> {
    let trimmed = stdout.trim();
    if !trimmed.starts_with('{') {
        return None;
    }

    let value: Value = serde_json::from_str(trimmed).ok()?;
    let object = value.as_object()?;

    let continue_flag = object.get("continue").and_then(|v| v.as_bool());
    let decision = object
        .get("decision")
        .and_then(|v| v.as_str())
        .map(|s| s.eq_ignore_ascii_case("block"))
        .unwrap_or(false);
    let blocked = continue_flag.map(|v| !v).unwrap_or(false) || decision;

    let reason = object
        .get("reason")
        .or_else(|| object.get("stopReason"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let modified_input = object
        .get("modified_input")
        .cloned()
        .or_else(|| object.get("updatedInput").cloned())
        .or_else(|| {
            object
                .get("hookSpecificOutput")
                .and_then(|v| v.get("updatedInput"))
                .cloned()
        });

    let stdout = collect_hook_text_outputs(object);
    let wake_notification = object
        .get("wakeNotification")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            object
                .get("hookSpecificOutput")
                .and_then(|v| v.get("wakeNotification"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });
    let memory_sections = object
        .get("hookSpecificOutput")
        .and_then(|v| v.get("memorySections"))
        .and_then(render_memory_sections_markdown);
    let stdout = merge_hook_output_parts(stdout, memory_sections);

    Some(HookResult {
        blocked,
        reason,
        modified_input,
        stdout,
        wake_notification,
    })
}

fn collect_hook_text_outputs(object: &serde_json::Map<String, Value>) -> Option<String> {
    let mut parts = Vec::new();
    push_unique_output(
        &mut parts,
        object
            .get("systemMessage")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    );
    push_unique_output(
        &mut parts,
        object
            .get("additional_context")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    );
    push_unique_output(
        &mut parts,
        object
            .get("hookSpecificOutput")
            .and_then(|v| v.get("additionalContext"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    );

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

fn merge_hook_output_parts(primary: Option<String>, secondary: Option<String>) -> Option<String> {
    let mut parts = Vec::new();
    push_unique_output(&mut parts, primary);
    push_unique_output(&mut parts, secondary);
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

fn push_unique_output(target: &mut Vec<String>, value: Option<String>) {
    let Some(value) = value else {
        return;
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    if !target.iter().any(|existing| existing == trimmed) {
        target.push(trimmed.to_string());
    }
}

fn render_memory_sections_markdown(value: &Value) -> Option<String> {
    let object = value.as_object()?;
    let mut lines = Vec::new();
    let sections = [
        ("goals", "Goals"),
        ("findings", "Findings"),
        ("decisions", "Decisions"),
        ("files", "Files"),
        ("open_questions", "Open Questions"),
        ("freshness", "Freshness"),
        ("confidence", "Confidence"),
    ];

    for (key, title) in sections {
        let Some(items) = object.get(key).and_then(|v| v.as_array()) else {
            continue;
        };
        lines.push(format!("### {}", title));
        lines.push(String::new());
        if items.is_empty() {
            lines.push("- None".to_string());
        } else {
            for item in items {
                if let Some(text) = item.as_str() {
                    lines.push(format!("- {}", text));
                }
            }
        }
        lines.push(String::new());
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n").trim().to_string())
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
        assert_eq!(HookEvent::PreCompact.to_string(), "pre_compact");
        assert_eq!(HookEvent::PostCompact.to_string(), "post_compact");
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
            metadata: None,
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
            metadata: None,
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
            metadata: None,
        };
        let results = mgr.execute(HookEvent::PreToolUse, &ctx).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_hook_manager_parses_structured_json_output() {
        let mut mgr = HookManager::new(PathBuf::from("/tmp"));
        mgr.register(HookDefinition {
            command: "printf '%s' '{\"continue\":false,\"reason\":\"blocked\",\"modified_input\":{\"path\":\"src/main.rs\"},\"systemMessage\":\"hook context\"}'".into(),
            events: vec!["pre_tool_use".into()],
            tool_filter: None,
            timeout_secs: 5,
            can_block: true,
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
            metadata: None,
        };
        let results = mgr.execute(HookEvent::PreToolUse, &ctx).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].blocked);
        assert_eq!(results[0].reason.as_deref(), Some("blocked"));
        assert_eq!(results[0].stdout.as_deref(), Some("hook context"));
        assert_eq!(
            results[0]
                .modified_input
                .as_ref()
                .and_then(|v| v.get("path"))
                .and_then(|v| v.as_str()),
            Some("src/main.rs")
        );
    }

    #[tokio::test]
    async fn test_hook_manager_queues_wake_notifications() {
        let mut mgr = HookManager::new(PathBuf::from("/tmp"));
        mgr.register(HookDefinition {
            command: "printf '%s' '{\"hookSpecificOutput\":{\"wakeNotification\":\"wake up\"}}' && exit 2".into(),
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
            metadata: None,
        };
        let _ = mgr.execute(HookEvent::PreToolUse, &ctx).await;
        let wake = mgr.drain_wake_notifications();
        assert_eq!(wake.len(), 1);
        assert_eq!(wake[0].message, "wake up");
        assert_eq!(wake[0].event, "pre_tool_use");
        let stats = mgr.stats_snapshot();
        assert_eq!(stats.total_executions, 1);
        assert_eq!(stats.wake_notification_count, 1);
    }

    #[tokio::test]
    async fn test_hook_manager_records_timeout_stats() {
        let mut mgr = HookManager::new(PathBuf::from("/tmp"));
        mgr.register(HookDefinition {
            command: "sleep 2".into(),
            events: vec!["pre_tool_use".into()],
            tool_filter: None,
            timeout_secs: 1,
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
            metadata: None,
        };
        let _ = mgr.execute(HookEvent::PreToolUse, &ctx).await;
        let stats = mgr.stats_snapshot();
        assert_eq!(stats.total_executions, 1);
        assert_eq!(stats.timeout_count, 1);
        assert_eq!(stats.last_timeout_command.as_deref(), Some("sleep 2"));
    }

    #[test]
    fn test_parse_structured_hook_output_supports_memory_sections() {
        let output = parse_structured_hook_output(
            "{\"hookSpecificOutput\":{\"memorySections\":{\"goals\":[\"Goal one\"],\"findings\":[\"Finding one\"],\"confidence\":[\"Medium\"]}}}",
        )
        .unwrap();
        let stdout = output.stdout.unwrap();
        assert!(stdout.contains("### Goals"));
        assert!(stdout.contains("- Goal one"));
        assert!(stdout.contains("### Findings"));
        assert!(stdout.contains("- Finding one"));
        assert!(stdout.contains("### Confidence"));
    }

    #[test]
    fn test_parse_structured_hook_output_merges_text_outputs_in_order() {
        let output = parse_structured_hook_output(
            "{\"systemMessage\":\"primary\",\"additional_context\":\"secondary\",\"hookSpecificOutput\":{\"additionalContext\":\"tertiary\",\"memorySections\":{\"goals\":[\"Goal one\"]}}}",
        )
        .unwrap();
        let stdout = output.stdout.unwrap();
        let primary_idx = stdout.find("primary").unwrap();
        let secondary_idx = stdout.find("secondary").unwrap();
        let tertiary_idx = stdout.find("tertiary").unwrap();
        let goals_idx = stdout.find("### Goals").unwrap();
        assert!(primary_idx < secondary_idx);
        assert!(secondary_idx < tertiary_idx);
        assert!(tertiary_idx < goals_idx);
    }
}
