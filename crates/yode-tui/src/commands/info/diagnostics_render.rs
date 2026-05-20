use crate::runtime_display::{format_permission_decision_summary, format_tool_progress_summary};
use crate::runtime_timeline::build_runtime_timeline_lines_with_project_root;
use crate::ui::status_summary::{
    context_window_summary_text, runtime_status_snapshot_from_parts, session_runtime_summary_text,
    tool_runtime_summary_text,
};

pub(crate) fn render_diagnostics_overview(
    project_root: &std::path::Path,
    state: &yode_core::engine::EngineRuntimeState,
    tasks: &[yode_tools::RuntimeTask],
) -> String {
    let running_tasks = tasks
        .iter()
        .filter(|task| matches!(task.status, yode_tools::RuntimeTaskStatus::Running))
        .count();
    let runtime_snapshot =
        runtime_status_snapshot_from_parts(project_root, Some(state.clone()), running_tasks);
    let runtime_summary =
        session_runtime_summary_text(&runtime_snapshot, state.estimated_context_tokens);
    let context_summary = context_window_summary_text(Some(state), state.estimated_context_tokens);
    let tool_summary = tool_runtime_summary_text(state);
    let recent_denials = if state.recent_permission_denials.is_empty() {
        "none".to_string()
    } else {
        state.recent_permission_denials.join(" | ")
    };
    let tool_errors = if state.tool_error_type_counts.is_empty() {
        "none".to_string()
    } else {
        state
            .tool_error_type_counts
            .iter()
            .map(|(kind, count)| format!("{}={}", kind, count))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let permission_summary = format_permission_decision_summary(
        state.last_permission_tool.as_deref(),
        state.last_permission_action.as_deref(),
        state.last_permission_explanation.as_deref(),
    );
    let tool_progress_summary = format_tool_progress_summary(
        state.last_tool_progress_tool.as_deref(),
        state.last_tool_progress_message.as_deref(),
        state.last_tool_progress_at.as_deref(),
    );
    let timeline =
        build_runtime_timeline_lines_with_project_root(Some(project_root), state, tasks, 6)
            .into_iter()
            .map(|line| format!("  - {}", line))
            .collect::<Vec<_>>()
            .join("\n");
    let startup_settings =
        crate::commands::info::startup_artifacts::latest_settings_scopes(project_root)
            .map(|summary| {
                summary
                    .scopes
                    .into_iter()
                    .map(|scope| {
                        format!(
                            "{}:{} mcp={} rules={}",
                            scope.scope,
                            scope
                                .permission_default_mode
                                .unwrap_or_else(|| "inherit".to_string()),
                            scope.mcp_server_count,
                            scope.permission_rule_count
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" | ")
            })
            .unwrap_or_else(|| "none".to_string());
    let managed_mcp =
        crate::commands::info::startup_artifacts::latest_managed_mcp_inventory(project_root)
            .map(|summary| {
                format!(
                    "effective={} configured={} connected={} tools={} failures={}",
                    summary.effective_server_count,
                    summary.configured_server_count,
                    summary.connected_server_count,
                    summary.mcp_tool_count,
                    summary.failure_count
                )
            })
            .unwrap_or_else(|| "none".to_string());
    let mcp_resource_artifacts =
        crate::commands::tools::mcp_workspace::mcp_resource_artifact_summary(project_root);
    let plugin_summary = plugin_inventory_summary(project_root);
    let skill_summary = skill_diagnostics_summary(project_root);
    let team_state = crate::commands::artifact_nav::latest_agent_team_state_artifact(project_root)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "none".to_string());
    let remote_live =
        crate::commands::artifact_nav::latest_remote_live_session_state_artifact(project_root)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string());
    let hook_defer =
        crate::commands::artifact_nav::latest_hook_deferred_state_artifact(project_root)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string());
    let issue_summary = render_diagnostic_issue_summary(project_root, state, tasks);
    let progress_line = format!(
        "{} ({})",
        state.tool_progress_event_count, tool_progress_summary
    );

    format!(
        "Diagnostics overview:\n{}\n\n  Runtime summary: {}\n  Context summary: {}\n  Tool summary:    {}\n\nContext:\n  Query source:   {}\n  Compact count:  {} (auto {}, manual {})\n  Breaker reason: {}\n  Compact tokens: {}\n  Media compact:  last {} / total {} removed, saved ~{} chars\n\nMemory:\n  Live memory:    {}{}\n  Memory updates: {}\n  Last memory:    {}\n\nRecovery:\n  State:          {}\n  Last signature: {}\n  Permission:     {}\n  Denials:        {}\n\nTools:\n  Session calls:  {}\n  Progress:       {}\n  Parallel:       {} batches / {} calls\n  Truncations:    {}\n  Errors:         {}\n  Last artifact:  {}\n\nObservability:\n  Hook defer:     {}\n  Agent team:     {}\n  Remote live:    {}\n  Settings:       {}\n  Managed MCP:    {}\n  MCP resources:  {}\n  Plugins:        {}\n  Skills:         {}\n\nTasks:\n  Total:          {}\n  Running:        {}\n\nHooks:\n  Total runs:     {}\n  Timeouts:       {}\n  Wake notices:   {}\n\nTimeline:\n{}",
        issue_summary,
        runtime_summary,
        context_summary,
        tool_summary,
        state.query_source,
        state.total_compactions,
        state.auto_compactions,
        state.manual_compactions,
        state
            .last_compaction_breaker_reason
            .as_deref()
            .unwrap_or("none"),
        state
            .last_compaction_prompt_tokens
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        state.last_microcompact_media_removed,
        state.microcompact_media_removed_total,
        state.microcompact_media_saved_chars_total,
        if state.live_session_memory_initialized {
            "warm"
        } else {
            "cold"
        },
        if state.live_session_memory_updating {
            " (updating)"
        } else {
            ""
        },
        state.session_memory_update_count,
        state
            .last_session_memory_update_path
            .as_deref()
            .unwrap_or("none"),
        state.recovery_state,
        state.last_failed_signature.as_deref().unwrap_or("none"),
        permission_summary,
        recent_denials,
        state.session_tool_calls_total,
        progress_line,
        state.parallel_tool_batch_count,
        state.parallel_tool_call_count,
        state.tool_truncation_count,
        tool_errors,
        state
            .last_tool_turn_artifact_path
            .as_deref()
            .unwrap_or("none"),
        hook_defer,
        team_state,
        remote_live,
        startup_settings,
        managed_mcp,
        mcp_resource_artifacts,
        plugin_summary,
        skill_summary,
        tasks.len(),
        running_tasks,
        state.hook_total_executions,
        state.hook_timeout_count,
        state.hook_wake_notification_count,
        timeline,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DiagnosticSeverity {
    Critical,
    Warning,
    Info,
}

impl DiagnosticSeverity {
    fn label(self) -> &'static str {
        match self {
            Self::Critical => "Critical",
            Self::Warning => "Warning",
            Self::Info => "Info",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiagnosticIssue {
    severity: DiagnosticSeverity,
    title: String,
    detail: String,
    action: &'static str,
}

fn render_diagnostic_issue_summary(
    project_root: &std::path::Path,
    state: &yode_core::engine::EngineRuntimeState,
    tasks: &[yode_tools::RuntimeTask],
) -> String {
    let issues = diagnostic_issues(project_root, state, tasks);
    render_diagnostic_issue_lines(&issues, 96).join("\n")
}

fn render_diagnostic_issue_lines(issues: &[DiagnosticIssue], max_width: usize) -> Vec<String> {
    if issues.is_empty() {
        return vec![
            "  Issues:         none".to_string(),
            "  Next action:    continue current work; use /status for full runtime detail"
                .to_string(),
        ];
    }

    let critical = issues
        .iter()
        .filter(|issue| issue.severity == DiagnosticSeverity::Critical)
        .count();
    let warning = issues
        .iter()
        .filter(|issue| issue.severity == DiagnosticSeverity::Warning)
        .count();
    let info = issues
        .iter()
        .filter(|issue| issue.severity == DiagnosticSeverity::Info)
        .count();

    let mut lines = vec![format!(
        "  Issues:         critical={} warning={} info={}",
        critical, warning, info
    )];
    let mut previous = None;
    for issue in issues.iter().take(5) {
        if previous != Some(issue.severity) {
            lines.push(format!("  {}:", issue.severity.label()));
            previous = Some(issue.severity);
        }
        lines.push(truncate_visible_width(
            &format!(
                "    - {}: {} -> {}",
                issue.title, issue.detail, issue.action
            ),
            max_width,
        ));
    }

    lines
}

fn diagnostic_issues(
    project_root: &std::path::Path,
    state: &yode_core::engine::EngineRuntimeState,
    tasks: &[yode_tools::RuntimeTask],
) -> Vec<DiagnosticIssue> {
    let mut issues = Vec::new();

    if state.recovery_state != "Normal" {
        issues.push(DiagnosticIssue {
            severity: DiagnosticSeverity::Critical,
            title: "Recovery".to_string(),
            detail: format!("state={}", state.recovery_state),
            action: "/status recovery",
        });
    }
    if state.hook_timeout_count > 0 || state.hook_execution_error_count > 0 {
        issues.push(DiagnosticIssue {
            severity: DiagnosticSeverity::Critical,
            title: "Hooks".to_string(),
            detail: format!(
                "timeouts={} errors={}",
                state.hook_timeout_count, state.hook_execution_error_count
            ),
            action: "/hooks",
        });
    }
    if !state.recent_permission_denials.is_empty() {
        issues.push(DiagnosticIssue {
            severity: DiagnosticSeverity::Critical,
            title: "Permissions".to_string(),
            detail: format!("{} recent denial(s)", state.recent_permission_denials.len()),
            action: "/permissions recovery",
        });
    }

    let plugin_errors = plugin_manifest_diagnostic_count(project_root);
    if plugin_errors > 0 {
        issues.push(DiagnosticIssue {
            severity: DiagnosticSeverity::Warning,
            title: "Plugins".to_string(),
            detail: format!("{} manifest diagnostic(s)", plugin_errors),
            action: "/plugin list",
        });
    }
    let skill_errors = skill_diagnostic_count(project_root);
    if skill_errors > 0 {
        issues.push(DiagnosticIssue {
            severity: DiagnosticSeverity::Warning,
            title: "Skills".to_string(),
            detail: format!("{} stale reference(s)", skill_errors),
            action: "/skills list",
        });
    }
    if state.compaction_failures > 0 {
        issues.push(DiagnosticIssue {
            severity: DiagnosticSeverity::Warning,
            title: "Compaction".to_string(),
            detail: format!("{} failure(s)", state.compaction_failures),
            action: "/compact",
        });
    } else if state.compaction_threshold_tokens > 0
        && state.estimated_context_tokens >= state.compaction_threshold_tokens
    {
        issues.push(DiagnosticIssue {
            severity: DiagnosticSeverity::Warning,
            title: "Context".to_string(),
            detail: format!(
                "{} / {} tokens",
                state.estimated_context_tokens, state.compaction_threshold_tokens
            ),
            action: "/compact",
        });
    }
    if state.tool_truncation_count > 0 {
        issues.push(DiagnosticIssue {
            severity: DiagnosticSeverity::Warning,
            title: "Tools".to_string(),
            detail: format!("{} truncated result(s)", state.tool_truncation_count),
            action: "/tools diagnostics",
        });
    }
    if !state.tool_error_type_counts.is_empty() {
        issues.push(DiagnosticIssue {
            severity: DiagnosticSeverity::Warning,
            title: "Tool errors".to_string(),
            detail: state
                .tool_error_type_counts
                .iter()
                .map(|(kind, count)| format!("{}={}", kind, count))
                .collect::<Vec<_>>()
                .join(", "),
            action: "/tools diagnostics",
        });
    }

    let running_tasks = tasks
        .iter()
        .filter(|task| matches!(task.status, yode_tools::RuntimeTaskStatus::Running))
        .count();
    if running_tasks > 0 {
        issues.push(DiagnosticIssue {
            severity: DiagnosticSeverity::Info,
            title: "Tasks".to_string(),
            detail: format!("{} running", running_tasks),
            action: "/tasks summary",
        });
    }
    if state.tool_progress_event_count > 0 {
        issues.push(DiagnosticIssue {
            severity: DiagnosticSeverity::Info,
            title: "Tool progress".to_string(),
            detail: format!("{} event(s)", state.tool_progress_event_count),
            action: "/tools diagnostics",
        });
    }

    issues.sort_by_key(|issue| issue.severity);
    issues
}

fn plugin_manifest_diagnostic_count(project_root: &std::path::Path) -> usize {
    yode_core::plugins::PluginRegistry::discover(project_root)
        .diagnostics()
        .len()
}

fn skill_diagnostic_count(project_root: &std::path::Path) -> usize {
    yode_core::skills::SkillRegistry::discover(&yode_core::skills::SkillRegistry::default_paths(
        project_root,
    ))
    .diagnostics()
    .len()
}

fn truncate_visible_width(value: &str, max_width: usize) -> String {
    if unicode_width::UnicodeWidthStr::width(value) <= max_width {
        return value.to_string();
    }

    let suffix = "...";
    let limit = max_width.saturating_sub(suffix.len());
    let mut rendered = String::new();
    let mut width = 0;
    for ch in value.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > limit {
            break;
        }
        rendered.push(ch);
        width += ch_width;
    }
    rendered.push_str(suffix);
    rendered
}

pub(crate) fn plugin_inventory_summary(project_root: &std::path::Path) -> String {
    let registry = yode_core::plugins::PluginRegistry::discover(project_root);
    let mut installed = 0;
    let mut enabled = 0;
    let mut disabled = 0;
    let mut blocked = 0;
    for plugin in registry.plugins() {
        match plugin.trust {
            yode_core::plugins::PluginTrustState::Installed => installed += 1,
            yode_core::plugins::PluginTrustState::Enabled => enabled += 1,
            yode_core::plugins::PluginTrustState::Disabled => disabled += 1,
            yode_core::plugins::PluginTrustState::Blocked => blocked += 1,
        }
    }
    let errors = registry.diagnostics().len();
    let first_error = registry.diagnostics().first().map(|diagnostic| {
        format!(
            " first={} {}",
            diagnostic.manifest_path.display(),
            diagnostic.message
        )
    });
    format!(
        "plugins={} installed={} enabled={} disabled={} blocked={} manifest_errors={}{}",
        registry.plugins().len(),
        installed,
        enabled,
        disabled,
        blocked,
        errors,
        first_error.unwrap_or_default()
    )
}

pub(crate) fn skill_diagnostics_summary(project_root: &std::path::Path) -> String {
    let registry = yode_core::skills::SkillRegistry::discover(
        &yode_core::skills::SkillRegistry::default_paths(project_root),
    );
    let diagnostics = registry.diagnostics();
    let first = diagnostics.first().map(|diagnostic| {
        format!(
            " first={} {}",
            diagnostic.path.display(),
            diagnostic.message
        )
    });
    format!(
        "skills={} stale_refs={}{}",
        registry.list().len(),
        diagnostics.len(),
        first.unwrap_or_default()
    )
}

pub(crate) fn plugin_manifest_diagnostic_lines(project_root: &std::path::Path) -> Vec<String> {
    yode_core::plugins::PluginRegistry::discover(project_root)
        .diagnostics()
        .iter()
        .map(|diagnostic| {
            format!(
                "{}: {}",
                diagnostic.manifest_path.display(),
                diagnostic.message
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use yode_core::engine::{EngineRuntimeState, PromptCacheRuntimeState};
    use yode_core::tool_runtime::ToolRuntimeCallView;
    use yode_tools::registry::ToolPoolSnapshot;
    use yode_tools::{RuntimeTask, RuntimeTaskStatus};

    use super::{
        diagnostic_issues, render_diagnostic_issue_lines, render_diagnostics_overview,
        truncate_visible_width, DiagnosticIssue, DiagnosticSeverity,
    };

    fn state() -> EngineRuntimeState {
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
            last_compact_boundary: None,
            last_session_memory_update_at: None,
            last_session_memory_update_path: None,
            last_session_memory_generated_summary: false,
            session_memory_update_count: 0,
            tracked_failed_tool_results: 0,
            hook_total_executions: 0,
            hook_timeout_count: 0,
            hook_execution_error_count: 0,
            hook_nonzero_exit_count: 0,
            hook_wake_notification_count: 0,
            stop_hook_continue_count: 0,
            last_stop_hook_continue_reason: None,
            last_hook_failure_event: None,
            last_hook_failure_command: None,
            last_hook_failure_reason: None,
            last_hook_failure_at: None,
            last_hook_timeout_command: None,
            last_compaction_prompt_tokens: None,
            last_post_compaction_estimated_tokens: None,
            last_post_compaction_threshold_tokens: None,
            last_post_compaction_will_retrigger: None,
            last_restore_budget: None,
            plan: Default::default(),
            async_task_restore_summary: None,
            context_collapse: Default::default(),
            avg_compaction_prompt_tokens: None,
            compaction_cause_histogram: BTreeMap::new(),
            last_microcompact_media_removed: 0,
            last_microcompact_media_saved_chars: 0,
            microcompact_media_removed_total: 0,
            microcompact_media_saved_chars_total: 0,
            system_prompt_estimated_tokens: 0,
            system_prompt_segments: Vec::new(),
            prompt_cache: PromptCacheRuntimeState::default(),
            cost: Default::default(),
            last_turn_duration_ms: None,
            last_turn_stop_reason: None,
            last_turn_artifact_path: None,
            last_stream_watchdog_stage: None,
            stream_retry_reason_histogram: BTreeMap::new(),
            recovery_state: "Normal".to_string(),
            recovery_single_step_count: 0,
            recovery_reanchor_count: 0,
            recovery_need_user_guidance_count: 0,
            last_failed_signature: None,
            recovery_breadcrumbs: Vec::new(),
            last_recovery_artifact_path: None,
            last_permission_tool: None,
            last_permission_action: None,
            last_permission_explanation: None,
            last_permission_artifact_path: None,
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
    fn diagnostics_overview_mentions_extended_observability_artifacts() {
        let dir = std::env::temp_dir().join(format!(
            "yode-diagnostics-extended-{}",
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".yode").join("hooks")).unwrap();
        std::fs::create_dir_all(dir.join(".yode").join("teams")).unwrap();
        std::fs::create_dir_all(dir.join(".yode").join("remote")).unwrap();
        std::fs::create_dir_all(dir.join(".yode").join("startup")).unwrap();
        std::fs::create_dir_all(dir.join(".yode").join("plugins").join("broken")).unwrap();
        std::fs::create_dir_all(dir.join(".yode").join("status").join("mcp-resources")).unwrap();
        std::fs::write(
            dir.join(".yode")
                .join("hooks")
                .join("a-hook-deferred-state.json"),
            "{}",
        )
        .unwrap();
        std::fs::write(
            dir.join(".yode")
                .join("teams")
                .join("a-agent-team-state.json"),
            "{}",
        )
        .unwrap();
        std::fs::write(
            dir.join(".yode")
                .join("remote")
                .join("a-remote-live-session-state.json"),
            "{}",
        )
        .unwrap();
        std::fs::write(
            dir.join(".yode")
                .join("startup")
                .join("a-settings-scopes.json"),
            r#"{"scopes":[]}"#,
        )
        .unwrap();
        std::fs::write(dir.join(".yode").join("startup").join("a-managed-mcp-inventory.json"), r#"{"effective_server_count":1,"configured_server_count":1,"connected_server_count":1,"mcp_tool_count":2,"failure_count":0}"#).unwrap();
        std::fs::write(
            dir.join(".yode")
                .join("status")
                .join("mcp-resources")
                .join("a-mcp-resource.md"),
            "manifest",
        )
        .unwrap();
        let rendered = render_diagnostics_overview(&dir, &state(), &[]);
        assert!(rendered.contains("Hook defer:"));
        assert!(rendered.contains("Agent team:"));
        assert!(rendered.contains("Remote live:"));
        assert!(rendered.contains("Managed MCP:"));
        assert!(rendered.contains("MCP resources:"));
        assert!(rendered.contains("Plugins:"));
        assert!(rendered.contains("manifest_errors=1"));
        assert!(rendered.contains("manifest=1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn diagnostics_overview_includes_shared_runtime_summaries() {
        let rendered = render_diagnostics_overview(std::path::Path::new("/tmp"), &state(), &[]);
        assert!(rendered.contains("Runtime summary:"));
        assert!(rendered.contains("Context summary:"));
        assert!(rendered.contains("Tool summary:"));
        assert!(rendered.contains("Issues:         none"));
    }

    #[test]
    fn diagnostic_issues_group_severity_and_action_hints() {
        let mut state = state();
        state.recovery_state = "SingleStepMode".to_string();
        state.recent_permission_denials = vec!["bash rm".to_string()];
        state.estimated_context_tokens = 96_500;
        state.compaction_threshold_tokens = 96_000;
        state.tool_truncation_count = 2;
        let tasks = vec![RuntimeTask {
            id: "task-1".to_string(),
            kind: "subagent".to_string(),
            source_tool: "agent".to_string(),
            description: "running".to_string(),
            status: RuntimeTaskStatus::Running,
            attempt: 1,
            retry_of: None,
            output_path: "/tmp/task.out".to_string(),
            transcript_path: None,
            created_at: "now".to_string(),
            started_at: Some("now".to_string()),
            completed_at: None,
            last_progress: None,
            last_progress_at: None,
            progress_history: Vec::new(),
            error: None,
        }];

        let issues = diagnostic_issues(std::path::Path::new("/tmp"), &state, &tasks);
        let rendered = render_diagnostic_issue_lines(&issues, 96).join("\n");

        assert!(rendered.contains("critical=2 warning=2 info=1"));
        assert!(rendered.contains("Critical:"));
        assert!(rendered.contains("Recovery: state=SingleStepMode -> /status recovery"));
        assert!(rendered.contains("Permissions: 1 recent denial(s) -> /permissions recovery"));
        assert!(rendered.contains("Warning:"));
        assert!(rendered.contains("Context: 96500 / 96000 tokens -> /compact"));
        assert!(rendered.contains("Tools: 2 truncated result(s) -> /tools diagnostics"));
        assert!(rendered.contains("Info:"));
        assert!(rendered.contains("Tasks: 1 running -> /tasks summary"));
    }

    #[test]
    fn diagnostic_issue_lines_limit_top_five_items() {
        let issues = vec![
            DiagnosticIssue {
                severity: DiagnosticSeverity::Critical,
                title: "A".to_string(),
                detail: "a".to_string(),
                action: "/a",
            },
            DiagnosticIssue {
                severity: DiagnosticSeverity::Critical,
                title: "B".to_string(),
                detail: "b".to_string(),
                action: "/b",
            },
            DiagnosticIssue {
                severity: DiagnosticSeverity::Warning,
                title: "C".to_string(),
                detail: "c".to_string(),
                action: "/c",
            },
            DiagnosticIssue {
                severity: DiagnosticSeverity::Warning,
                title: "D".to_string(),
                detail: "d".to_string(),
                action: "/d",
            },
            DiagnosticIssue {
                severity: DiagnosticSeverity::Info,
                title: "E".to_string(),
                detail: "e".to_string(),
                action: "/e",
            },
            DiagnosticIssue {
                severity: DiagnosticSeverity::Info,
                title: "F".to_string(),
                detail: "f".to_string(),
                action: "/f",
            },
        ];

        let rendered = render_diagnostic_issue_lines(&issues, 96).join("\n");

        assert!(rendered.contains("critical=2 warning=2 info=2"));
        assert!(rendered.contains("E: e -> /e"));
        assert!(!rendered.contains("F: f -> /f"));
    }

    #[test]
    fn diagnostic_issue_lines_truncate_cjk_at_visible_width() {
        let value = "    - 权限: 需要检查重复的中文权限拒绝原因 -> /permissions recovery";
        let rendered = truncate_visible_width(value, 40);

        assert!(unicode_width::UnicodeWidthStr::width(rendered.as_str()) <= 40);
        assert!(rendered.ends_with("..."));
    }
}
