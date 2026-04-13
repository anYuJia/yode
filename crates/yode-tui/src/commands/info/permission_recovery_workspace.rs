use yode_core::engine::EngineRuntimeState;
use yode_core::permission::{PermissionMode, PermissionRule, RuleBehavior, RuleSource};

use crate::commands::workspace_nav::{runtime_artifact_jump_targets, workspace_jump_inventory};
use crate::commands::workspace_text::{workspace_artifact_lines, workspace_bullets, WorkspaceText};
use crate::runtime_display::format_permission_decision_summary;

pub(crate) fn rule_source_badge(source: RuleSource) -> &'static str {
    match source {
        RuleSource::UserConfig => "[user]",
        RuleSource::ProjectConfig => "[project]",
        RuleSource::Session => "[session]",
        RuleSource::CliArg => "[cli]",
    }
}

pub(crate) fn suggestion_severity(suggestion: &str) -> &'static str {
    if suggestion.contains("allow rule") {
        "high"
    } else if suggestion.contains("scoped bash rule") {
        "medium"
    } else {
        "info"
    }
}

pub(crate) fn hook_timeline_narrative(state: &EngineRuntimeState) -> Vec<String> {
    let mut lines = vec![format!("total runs: {}", state.hook_total_executions)];
    if state.hook_timeout_count > 0 {
        lines.push(format!(
            "timeouts: {} ({})",
            state.hook_timeout_count,
            state.last_hook_timeout_command.as_deref().unwrap_or("none")
        ));
    }
    if state.hook_nonzero_exit_count > 0 || state.hook_execution_error_count > 0 {
        lines.push(format!(
            "last failure: {} [{}] {}",
            state.last_hook_failure_command.as_deref().unwrap_or("none"),
            state.last_hook_failure_event.as_deref().unwrap_or("none"),
            state.last_hook_failure_reason.as_deref().unwrap_or("none")
        ));
    }
    if state.hook_wake_notification_count > 0 {
        lines.push(format!(
            "wake notifications: {}",
            state.hook_wake_notification_count
        ));
    }
    lines
}

pub(crate) fn permission_recovery_jump_inventory(
    permission_artifact: Option<&str>,
    recovery_artifact: Option<&str>,
) -> String {
    workspace_jump_inventory(
        runtime_artifact_jump_targets(permission_artifact)
            .into_iter()
            .chain(runtime_artifact_jump_targets(recovery_artifact))
            .collect::<Vec<_>>(),
    )
}

pub(crate) fn permission_recovery_operator_guide() -> &'static str {
    "Operator guide: inspect `/permissions` for rule intent, `/hooks` for hook failures, and `/brief` for the latest recovery preview."
}

pub(crate) fn render_permission_workspace(
    mode: PermissionMode,
    confirmable_tools: &[&str],
    rules: &[PermissionRule],
    recent_denials: &[String],
    denial_prefixes: &[String],
    safe_prefixes: &str,
    confirmation_suggestions: &[String],
    runtime: &EngineRuntimeState,
) -> String {
    let mut rule_lines = rules
        .iter()
        .map(|rule| {
            format!(
                "{} {} {}{}",
                rule_source_badge(rule.source),
                rule.tool_name,
                match rule.behavior {
                    RuleBehavior::Allow => "allow",
                    RuleBehavior::Deny => "deny",
                    RuleBehavior::Ask => "ask",
                },
                rule.pattern
                    .as_ref()
                    .map(|pattern| format!(" ({})", pattern))
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>();
    if rule_lines.is_empty() {
        rule_lines.push("none".to_string());
    }

    let suggestion_lines = if confirmation_suggestions.is_empty() {
        vec!["none".to_string()]
    } else {
        confirmation_suggestions
            .iter()
            .map(|suggestion| format!("[{}] {}", suggestion_severity(suggestion), suggestion))
            .collect()
    };

    WorkspaceText::new("Permission and recovery workspace")
        .field("Mode", mode.to_string())
        .field(
            "Last decision",
            format_permission_decision_summary(
                runtime.last_permission_tool.as_deref(),
                runtime.last_permission_action.as_deref(),
                runtime.last_permission_explanation.as_deref(),
            ),
        )
        .field("Recovery state", runtime.recovery_state.clone())
        .field("Safe bash", safe_prefixes.to_string())
        .section(
            "Confirmable tools",
            if confirmable_tools.is_empty() {
                workspace_bullets(["none"])
            } else {
                workspace_bullets(confirmable_tools.iter().map(|tool| tool.to_string()))
            },
        )
        .section("Rules", workspace_bullets(rule_lines))
        .section(
            "Recent denials",
            if recent_denials.is_empty() {
                workspace_bullets(["none"])
            } else {
                workspace_bullets(recent_denials.to_vec())
            },
        )
        .section(
            "Denial prefixes",
            if denial_prefixes.is_empty() {
                workspace_bullets(["none"])
            } else {
                workspace_bullets(denial_prefixes.to_vec())
            },
        )
        .section("Suggestions", workspace_bullets(suggestion_lines))
        .section(
            "Artifacts",
            workspace_artifact_lines([
                (
                    "permission",
                    runtime
                        .last_permission_artifact_path
                        .as_deref()
                        .unwrap_or("none")
                        .to_string(),
                ),
                (
                    "recovery",
                    runtime
                        .last_recovery_artifact_path
                        .as_deref()
                        .unwrap_or("none")
                        .to_string(),
                ),
            ]),
        )
        .footer(permission_recovery_jump_inventory(
            runtime.last_permission_artifact_path.as_deref(),
            runtime.last_recovery_artifact_path.as_deref(),
        ))
        .render()
}

pub(crate) fn render_hook_workspace(
    state: &EngineRuntimeState,
    hook_artifact: Option<&str>,
) -> String {
    WorkspaceText::new("Hook failure workspace")
        .field("Failed at", state.last_hook_failure_at.as_deref().unwrap_or("none"))
        .field(
            "Inspector",
            hook_artifact.unwrap_or("none").to_string(),
        )
        .section("Timeline", workspace_bullets(hook_timeline_narrative(state)))
        .section(
            "Artifacts",
            workspace_artifact_lines([("hook", hook_artifact.unwrap_or("none"))]),
        )
        .footer(workspace_jump_inventory(runtime_artifact_jump_targets(hook_artifact)))
        .render()
}

pub(crate) fn render_recovery_workspace(state: &EngineRuntimeState) -> String {
    WorkspaceText::new("Recovery workspace")
        .field("State", state.recovery_state.clone())
        .field(
            "Last signature",
            state.last_failed_signature.as_deref().unwrap_or("none").to_string(),
        )
        .section(
            "Breadcrumbs",
            if state.recovery_breadcrumbs.is_empty() {
                workspace_bullets(["none"])
            } else {
                workspace_bullets(state.recovery_breadcrumbs.clone())
            },
        )
        .section(
            "Artifacts",
            workspace_artifact_lines([(
                "recovery",
                state
                    .last_recovery_artifact_path
                    .as_deref()
                    .unwrap_or("none")
                    .to_string(),
            )]),
        )
        .footer(permission_recovery_operator_guide())
        .render()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use yode_core::engine::{EngineRuntimeState, PromptCacheRuntimeState};
    use yode_core::permission::{PermissionRule, RuleBehavior, RuleSource};
    use yode_core::tool_runtime::ToolRuntimeCallView;
    use yode_tools::registry::ToolPoolSnapshot;

    use super::{
        hook_timeline_narrative, permission_recovery_operator_guide,
        render_hook_workspace, render_permission_workspace, render_recovery_workspace,
        rule_source_badge, suggestion_severity,
    };

    fn runtime_state() -> EngineRuntimeState {
        EngineRuntimeState {
            query_source: "User".to_string(),
            autocompact_disabled: false,
            compaction_failures: 0,
            total_compactions: 0,
            auto_compactions: 0,
            manual_compactions: 0,
            last_compaction_breaker_reason: None,
            context_window_tokens: 0,
            compaction_threshold_tokens: 0,
            estimated_context_tokens: 0,
            message_count: 0,
            live_session_memory_initialized: false,
            live_session_memory_updating: false,
            live_session_memory_path: String::new(),
            session_tool_calls_total: 0,
            last_compaction_mode: None,
            last_compaction_at: None,
            last_compaction_summary_excerpt: None,
            last_compaction_session_memory_path: None,
            last_compaction_transcript_path: None,
            last_session_memory_update_at: None,
            last_session_memory_update_path: None,
            last_session_memory_generated_summary: false,
            session_memory_update_count: 0,
            tracked_failed_tool_results: 0,
            hook_total_executions: 2,
            hook_timeout_count: 1,
            hook_execution_error_count: 0,
            hook_nonzero_exit_count: 1,
            hook_wake_notification_count: 1,
            last_hook_failure_event: Some("pre_tool".to_string()),
            last_hook_failure_command: Some("scripts/pre-tool".to_string()),
            last_hook_failure_reason: Some("exit 2".to_string()),
            last_hook_failure_at: Some("2026-01-01 00:00:00".to_string()),
            last_hook_timeout_command: Some("scripts/pre-tool".to_string()),
            last_compaction_prompt_tokens: None,
            avg_compaction_prompt_tokens: None,
            compaction_cause_histogram: BTreeMap::new(),
            system_prompt_estimated_tokens: 0,
            system_prompt_segments: Vec::new(),
            prompt_cache: PromptCacheRuntimeState::default(),
            last_turn_duration_ms: None,
            last_turn_stop_reason: None,
            last_turn_artifact_path: None,
            last_stream_watchdog_stage: None,
            stream_retry_reason_histogram: BTreeMap::new(),
            recovery_state: "SingleStepMode".to_string(),
            recovery_single_step_count: 0,
            recovery_reanchor_count: 0,
            recovery_need_user_guidance_count: 0,
            last_failed_signature: Some("bash".to_string()),
            recovery_breadcrumbs: vec!["parse".to_string(), "tool".to_string()],
            last_recovery_artifact_path: Some("/tmp/recovery.md".to_string()),
            last_permission_tool: Some("bash".to_string()),
            last_permission_action: Some("confirm".to_string()),
            last_permission_explanation: Some("needs approval".to_string()),
            last_permission_artifact_path: Some("/tmp/permission.json".to_string()),
            recent_permission_denials: Vec::new(),
            tool_pool: ToolPoolSnapshot::default(),
            current_turn_tool_calls: 0,
            current_turn_tool_output_bytes: 0,
            current_turn_tool_progress_events: 0,
            current_turn_parallel_batches: 0,
            current_turn_parallel_calls: 0,
            current_turn_max_parallel_batch_size: 0,
            current_turn_truncated_results: 0,
            current_turn_budget_notice_emitted: false,
            current_turn_budget_warning_emitted: false,
            tool_budget_notice_count: 0,
            tool_budget_warning_count: 0,
            last_tool_budget_warning: None,
            tool_progress_event_count: 0,
            last_tool_progress_message: None,
            last_tool_progress_tool: None,
            last_tool_progress_at: None,
            parallel_tool_batch_count: 0,
            parallel_tool_call_count: 0,
            max_parallel_batch_size: 0,
            tool_truncation_count: 0,
            last_tool_truncation_reason: None,
            latest_repeated_tool_failure: None,
            read_file_history: Vec::new(),
            command_tool_duplication_hints: Vec::new(),
            last_tool_turn_completed_at: None,
            last_tool_turn_artifact_path: None,
            tool_error_type_counts: BTreeMap::new(),
            tool_trace_scope: "last".to_string(),
            tool_traces: Vec::<ToolRuntimeCallView>::new(),
        }
    }

    #[test]
    fn rule_and_suggestion_helpers_render() {
        assert_eq!(rule_source_badge(RuleSource::CliArg), "[cli]");
        assert_eq!(suggestion_severity("consider allow rule"), "high");
        assert!(permission_recovery_operator_guide().contains("/permissions"));
    }

    #[test]
    fn hook_and_permission_workspaces_render() {
        let state = runtime_state();
        assert!(hook_timeline_narrative(&state)
            .iter()
            .any(|line| line.contains("timeouts")));
        let permission = render_permission_workspace(
            yode_core::PermissionMode::Default,
            &["bash"],
            &[PermissionRule {
                source: RuleSource::Session,
                behavior: RuleBehavior::Allow,
                tool_name: "bash".to_string(),
                pattern: None,
            }],
            &[],
            &[],
            "git status",
            &["consider allow rule".to_string()],
            &state,
        );
        assert!(permission.contains("Permission and recovery workspace"));
        let hook = render_hook_workspace(&state, Some("/tmp/hook.md"));
        assert!(hook.contains("Hook failure workspace"));
        let recovery = render_recovery_workspace(&state);
        assert!(recovery.contains("Recovery workspace"));
    }
}
