use crate::commands::context::CommandContext;
use crate::commands::registry::VisibleCommandName;
use crate::commands::info::startup_artifacts::{
    latest_mcp_startup_failures, latest_provider_inventory, latest_startup_manifest,
};
use crate::runtime_artifacts::write_runtime_timeline_artifact;
use crate::ui::status_summary::{
    context_window_summary_text, runtime_status_snapshot_from_parts,
    session_runtime_summary_text, tool_runtime_summary_text,
};
use yode_core::updater::{latest_local_release_tag, release_version_matches_tag, CURRENT_VERSION};
use super::shared::render_section;

pub(super) fn render_doctor_report(ctx: &mut CommandContext) -> String {
    let mut env_checks = Vec::new();
    let mut tooling_checks = Vec::new();
    let mut runtime_checks = Vec::new();
    let mut version_checks = Vec::new();
    let runtime = ctx.engine.try_lock().ok().map(|engine| {
        (
            engine.runtime_state(),
            engine.runtime_tasks_snapshot(),
            engine.permissions().mode(),
            engine.permissions().source_views_snapshot(),
            engine
                .permissions()
                .confirmable_tools()
                .into_iter()
                .map(|tool| tool.to_string())
                .collect::<Vec<_>>(),
            engine.permissions().recent_denial_prefixes(5),
            engine.permissions().safe_readonly_shell_prefixes().join(", "),
            engine.permissions().confirmation_rule_suggestions(3),
        )
    });
    let project_root = std::path::PathBuf::from(&ctx.session.working_dir);

    if ctx.all_provider_models.is_empty() {
        env_checks.push(
            "  [!!] No LLM providers configured. Run /provider add to set one up.".to_string(),
        );
    } else {
        let names: Vec<_> = ctx.all_provider_models.keys().cloned().collect();
        env_checks.push(format!(
            "  [ok] LLM providers configured: {}",
            names.join(", ")
        ));
    }

    env_checks.push(
        match std::process::Command::new("git").arg("--version").output() {
            Ok(output) if output.status.success() => format!(
                "  [ok] git available: {}",
                String::from_utf8_lossy(&output.stdout).trim()
            ),
            _ => "  [!!] git not found or failed".to_string(),
        },
    );

    for (command, arg) in [
        ("node", "--version"),
        ("python3", "--version"),
        ("go", "version"),
        ("cargo", "--version"),
    ] {
        let output = std::process::Command::new(command).arg(arg).output();
        match output {
            Ok(output) if output.status.success() => {
                env_checks.push(format!(
                    "  [ok] {} available: {}",
                    command,
                    String::from_utf8_lossy(&output.stdout).trim()
                ));
            }
            _ => env_checks.push(format!("  [--] {} not found (optional)", command)),
        }
    }

    env_checks.push(if ctx.terminal_caps.truecolor {
        "  [ok] Truecolor support enabled".to_string()
    } else {
        "  [--] No truecolor (using 256 colors)".to_string()
    });
    if ctx.terminal_caps.in_tmux {
        env_checks.push("  [--] Running inside tmux".to_string());
    }
    if ctx.terminal_caps.in_ssh {
        env_checks.push("  [--] Running over SSH".to_string());
    }

    let inventory = ctx.tools.inventory();
    tooling_checks.push(format!(
        "  [ok] tools: {} total / {} active / {} deferred",
        inventory.total_count, inventory.active_count, inventory.deferred_count
    ));
    tooling_checks.push(format!(
        "  [ok] mcp tools: {} active / {} deferred",
        inventory.mcp_active_count, inventory.mcp_deferred_count
    ));
    tooling_checks.push(format!(
        "  [ok] tool activations: {} (last: {})",
        inventory.activation_count,
        inventory.last_activated_tool.as_deref().unwrap_or("none")
    ));
    tooling_checks.push(format!(
        "  [ok] tool search: {} ({})",
        inventory.tool_search_enabled,
        inventory.tool_search_reason.as_deref().unwrap_or("no reason recorded")
    ));
    if inventory.duplicate_registration_count > 0 {
        tooling_checks.push(format!(
            "  [!!] Duplicate tool registrations blocked: {} ({})",
            inventory.duplicate_registration_count,
            inventory.duplicate_tool_names.join(", ")
        ));
    } else {
        tooling_checks.push("  [ok] No duplicate tool registrations observed".to_string());
    }
    let command_tool_overlaps = collect_command_tool_overlaps(
        &ctx.cmd_registry.visible_command_names(),
        &ctx.tools
            .list()
            .into_iter()
            .map(|tool| tool.name().to_string())
            .chain(
                ctx.tools
                    .list_deferred()
                    .into_iter()
                    .map(|(name, _)| name),
            )
            .collect::<Vec<_>>(),
    );
    if command_tool_overlaps.is_empty() {
        tooling_checks.push("  [ok] No command/tool naming overlaps detected".to_string());
    } else {
        tooling_checks.push(format!(
            "  [--] Command/tool naming overlaps: {}",
            command_tool_overlaps.join(", ")
        ));
    }
    if let Some(path) = dirs::home_dir().map(|home| home.join(".yode/config.toml")) {
        if path.exists() {
            env_checks.push(format!("  [ok] Config file: {:?}", path));
        } else {
            env_checks.push("  [!!] Config file missing".to_string());
        }
    }
    if let Some(profile) = ctx.session.startup_profile.as_deref() {
        env_checks.push(format!("  [ok] Startup profile: {}", profile));
    } else {
        env_checks.push("  [--] Startup profile unavailable".to_string());
    }
    if let Some(manifest) = latest_startup_manifest(&project_root) {
        env_checks.push(format!(
            "  [ok] Startup bundle manifest: {} ({} artifacts)",
            manifest.path.display(),
            manifest.artifact_count
        ));
    } else {
        env_checks.push("  [--] Startup bundle manifest unavailable".to_string());
    }
    if let Some(provider_inventory) = latest_provider_inventory(&project_root) {
        env_checks.push(format!(
            "  [ok] Provider inventory: {} (selected {} / {})",
            provider_inventory.path.display(),
            provider_inventory.provider_name,
            provider_inventory.model
        ));
        tooling_checks.push(format!(
            "  [ok] Provider source mix: {}",
            provider_inventory.source_breakdown.compact_label()
        ));
        if let Some(selected) = provider_inventory
            .provider_details
            .iter()
            .find(|detail| detail.name == ctx.provider_name.as_str())
        {
            tooling_checks.push(format!(
                "  [ok] Selected provider source: {} / models={} / {} / {} / {} ({})",
                selected.format,
                selected.model_count,
                selected.registration_source,
                selected.api_key_source,
                selected.base_url_source,
                selected.base_url
            ));
        }
    } else {
        env_checks.push("  [--] Provider inventory artifact unavailable".to_string());
    }
    if let Some(mcp_failures) = latest_mcp_startup_failures(&project_root) {
        tooling_checks.push(format!(
            "  [!!] MCP startup failures: {} (configured {}, connected {}, tools {})",
            mcp_failures.failure_count,
            mcp_failures.configured_server_count,
            mcp_failures.connected_server_count,
            mcp_failures.mcp_tool_count
        ));
        tooling_checks.push(format!(
            "  [!!] MCP failure artifact: {}",
            mcp_failures.path.display()
        ));
        let preview = mcp_failures
            .failures
            .iter()
            .take(2)
            .map(|failure| {
                format!(
                    "{} [{}]: {}",
                    failure.server, failure.phase, failure.message
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        tooling_checks.push(format!("  [!!] MCP failure preview: {}", preview));
    } else {
        tooling_checks.push("  [ok] MCP startup failures: none recorded".to_string());
    }

    if let Some((state, tasks, permission_mode, source_views, confirmable_tools, denial_prefixes, safe_prefixes, confirmation_suggestions)) = runtime {
        runtime_checks.extend(runtime_health_checks(
            &project_root,
            &ctx.session.session_id,
            &state,
            &tasks,
            permission_mode,
            &source_views,
            &confirmable_tools,
            &denial_prefixes
                .into_iter()
                .map(|entry| {
                    format!(
                        "{} x{} (consecutive {}, at {})",
                        entry.prefix, entry.count, entry.consecutive, entry.last_at
                    )
                })
                .collect::<Vec<_>>(),
            &safe_prefixes,
            &confirmation_suggestions,
        ));
    } else {
        runtime_checks.push("  [--] Engine runtime busy; skipped context/memory checks".to_string());
    }

    version_checks.push(match latest_local_release_tag() {
        Some(tag) if release_version_matches_tag(&tag, CURRENT_VERSION) => format!(
            "  [ok] Version matches latest local tag: {} == {}",
            CURRENT_VERSION, tag
        ),
        Some(tag) => format!(
            "  [!!] Version/tag mismatch: Cargo={} latest-tag={}",
            CURRENT_VERSION, tag
        ),
        None => "  [--] Could not determine latest local release tag".to_string(),
    });

    format!(
        "Yode Environment Health Check:\n\n{}{}{}{}\n  Platform: {} {}\n  Version:  v{}\n  Session:  {}",
        render_section("Environment", &env_checks),
        render_section("Tooling", &tooling_checks),
        render_section("Runtime", &runtime_checks),
        render_section("Version", &version_checks),
        std::env::consts::OS,
        std::env::consts::ARCH,
        env!("CARGO_PKG_VERSION"),
        &ctx.session.session_id[..8],
    )
}

fn collect_command_tool_overlaps(
    command_names: &[VisibleCommandName],
    tool_names: &[String],
) -> Vec<String> {
    let tool_set = tool_names
        .iter()
        .map(|name| name.to_lowercase())
        .collect::<std::collections::HashSet<_>>();
    let mut overlaps = command_names
        .iter()
        .filter(|item| tool_set.contains(&item.name.to_lowercase()))
        .map(|item| {
            if item.is_alias {
                format!("{} [alias]", item.name)
            } else {
                item.name.clone()
            }
        })
        .collect::<Vec<_>>();
    overlaps.sort();
    overlaps.dedup();
    overlaps
}

fn runtime_health_checks(
    project_root: &std::path::Path,
    session_id: &str,
    state: &yode_core::engine::EngineRuntimeState,
    tasks: &[yode_tools::RuntimeTask],
    permission_mode: yode_core::PermissionMode,
    source_views: &[yode_core::permission::PermissionSourceView],
    confirmable_tools: &[String],
    denial_prefixes: &[String],
    safe_prefixes: &str,
    confirmation_suggestions: &[String],
) -> Vec<String> {
    let mut checks = Vec::new();
    let benchmark = crate::commands::info::run_long_session_benchmark(project_root);
    let cache_stats = crate::commands::info::transcript_cache_stats();
    let running_tasks = tasks
        .iter()
        .filter(|task| matches!(task.status, yode_tools::RuntimeTaskStatus::Running))
        .count();
    let runtime_snapshot =
        runtime_status_snapshot_from_parts(project_root, Some(state.clone()), running_tasks);
    checks.push(format!(
        "  [ok] Runtime summary: {}",
        session_runtime_summary_text(&runtime_snapshot, state.estimated_context_tokens)
    ));
    checks.push(format!(
        "  [ok] Context summary: {}",
        context_window_summary_text(Some(state), state.estimated_context_tokens)
    ));
    checks.push(format!(
        "  [ok] Tool summary: {}",
        tool_runtime_summary_text(state)
    ));
    checks.push(format!(
        "  [ok] Compact count: {} (auto {}, manual {})",
        state.total_compactions, state.auto_compactions, state.manual_compactions
    ));
    if state.autocompact_disabled {
        checks.push(format!(
            "  [!!] Autocompact breaker open: {}",
            state
                .last_compaction_breaker_reason
                .as_deref()
                .unwrap_or("unknown reason")
        ));
    } else {
        checks.push("  [ok] Autocompact breaker closed".to_string());
    }

    let live_path = yode_core::session_memory::live_session_memory_path(project_root);
    checks.push(if live_path.exists() {
        format!("  [ok] Live memory file present: {}", live_path.display())
    } else {
        format!("  [--] Live memory file missing: {}", live_path.display())
    });

    let session_path = yode_core::session_memory::session_memory_path(project_root);
    checks.push(if session_path.exists() {
        format!(
            "  [ok] Session memory file present: {}",
            session_path.display()
        )
    } else {
        format!(
            "  [--] Session memory file missing: {}",
            session_path.display()
        )
    });

    let transcripts_dir = project_root.join(".yode").join("transcripts");
    let transcript_count = std::fs::read_dir(&transcripts_dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .count();
    checks.push(format!(
        "  [ok] Transcript artifacts visible: {}",
        transcript_count
    ));
    checks.push(format!(
        "  [ok] Transcript bench latest lookup: cold {} ms / hot {} ms",
        benchmark.cold_latest_lookup_ms, benchmark.hot_latest_lookup_ms
    ));
    checks.push(format!(
        "  [ok] Transcript bench failed filter: cold {} ms / hot {} ms",
        benchmark.cold_failed_filter_ms, benchmark.hot_failed_filter_ms
    ));
    checks.push(format!(
        "  [ok] Transcript bench resume warmup: {} ms ({} metadata, latest={})",
        benchmark.resume_warmup.duration_ms,
        benchmark.resume_warmup.metadata_entries_warmed,
        if benchmark.resume_warmup.latest_lookup_cached {
            "yes"
        } else {
            "no"
        }
    ));
    checks.push(format!(
        "  [ok] Transcript cache stats: metadata {} hit / {} miss, latest {} hit / {} miss",
        cache_stats.metadata_hits,
        cache_stats.metadata_misses,
        cache_stats.latest_hits,
        cache_stats.latest_misses
    ));
    checks.push(format!(
        "  [ok] Transcript cache invalidations: {} ({})",
        cache_stats.invalidations,
        cache_stats
            .last_invalidation_reason
            .as_deref()
            .unwrap_or("none")
    ));
    checks.push(format!(
        "  [ok] Session memory updates recorded: {}",
        state.session_memory_update_count
    ));
    checks.push(format!(
        "  [ok] Failed tool results tracked: {}",
        state.tracked_failed_tool_results
    ));
    checks.push(format!(
        "  [ok] tool pool: {} active visible / {} active hidden / {} deferred visible / {} deferred hidden",
        state.tool_pool.visible_active_count(),
        state.tool_pool.hidden_active_count(),
        state.tool_pool.visible_deferred_count(),
        state.tool_pool.hidden_deferred_count()
    ));
    checks.push(format!(
        "  [ok] tool pool policy: mode={} confirm={} deny={}",
        state.tool_pool.permission_mode,
        state.tool_pool.confirm_count(),
        state.tool_pool.deny_count()
    ));
    if source_views.is_empty() {
        checks.push("  [--] Permission scopes: none recorded".to_string());
    } else {
        let scope_summary = source_views
            .iter()
            .map(|view| {
                format!(
                    "{:?}(mode={},rules={})",
                    view.source,
                    view.default_mode.as_deref().unwrap_or("inherit"),
                    view.rules.len()
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        checks.push(format!("  [ok] Permission scopes: {}", scope_summary));
    }
    checks.push(format!("  [ok] Safe bash readonly prefixes: {}", safe_prefixes));
    if denial_prefixes.is_empty() {
        checks.push("  [ok] No bash denial prefixes recorded".to_string());
    } else {
        checks.push(format!(
            "  [--] Bash denial prefixes: {}",
            denial_prefixes.join(" | ")
        ));
    }
    if confirmation_suggestions.is_empty() {
        checks.push("  [ok] No repeated confirmation suggestions".to_string());
    } else {
        checks.push(format!(
            "  [--] Repeated confirmation suggestions: {}",
            confirmation_suggestions.join(" | ")
        ));
    }
    checks.push(format!(
        "  [ok] Tool progress events tracked: {}",
        state.tool_progress_event_count
    ));
    checks.push(format!(
        "  [ok] Parallel tool batches tracked: {}",
        state.parallel_tool_batch_count
    ));
    if let Some(path) = write_runtime_timeline_artifact(project_root, session_id, state, tasks) {
        checks.push(format!("  [ok] Runtime timeline artifact: {}", path));
    } else {
        checks.push("  [--] Runtime timeline artifact unavailable".to_string());
    }

    if state.tool_truncation_count > 0 {
        checks.push(format!(
            "  [!!] Tool truncations observed: {} (last: {})",
            state.tool_truncation_count,
            state
                .last_tool_truncation_reason
                .as_deref()
                .unwrap_or("unknown")
        ));
    } else {
        checks.push("  [ok] No tool truncations observed".to_string());
    }
    if let Some(pattern) = state.latest_repeated_tool_failure.as_deref() {
        checks.push(format!("  [!!] Repeated tool failure pattern: {}", pattern));
    } else {
        checks.push("  [ok] No repeated tool failure pattern observed".to_string());
    }
    if let Some(path) = state.last_tool_turn_artifact_path.as_deref() {
        checks.push(format!("  [ok] Tool artifact available: {}", path));
    } else {
        checks.push("  [--] Tool artifact not written yet".to_string());
    }
    checks.push(format!(
        "  [ok] Hook executions tracked: {}",
        state.hook_total_executions
    ));
    if state.recovery_state != "Normal" {
        checks.push(format!(
            "  [!!] Recovery state active: {} (signature: {})",
            state.recovery_state,
            state.last_failed_signature.as_deref().unwrap_or("none")
        ));
    } else {
        checks.push("  [ok] Recovery state normal".to_string());
    }
    if state.hook_timeout_count > 0 {
        checks.push(format!(
            "  [!!] Hook timeouts observed: {} (last: {})",
            state.hook_timeout_count,
            state
                .last_hook_timeout_command
                .as_deref()
                .unwrap_or("unknown")
        ));
    } else {
        checks.push("  [ok] No hook timeouts observed".to_string());
    }

    if matches!(permission_mode, yode_core::PermissionMode::Bypass) {
        checks.push(
            "  [!!] Permission mode is bypass — destructive tools are fully unlocked".to_string(),
        );
    } else {
        checks.push(format!("  [ok] Permission mode: {}", permission_mode));
    }

    for critical_tool in ["bash", "write_file", "edit_file"] {
        if confirmable_tools.iter().any(|tool| tool == critical_tool) {
            checks.push(format!(
                "  [ok] {} still requires confirmation",
                critical_tool
            ));
        } else {
            checks.push(format!(
                "  [!!] {} no longer requires confirmation",
                critical_tool
            ));
        }
    }

    checks
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use yode_core::engine::{EngineRuntimeState, PromptCacheRuntimeState};
    use yode_core::tool_runtime::ToolRuntimeCallView;
    use yode_tools::registry::ToolPoolSnapshot;

    use super::runtime_health_checks;

    fn state() -> EngineRuntimeState {
        EngineRuntimeState {
            query_source: "User".to_string(),
            autocompact_disabled: false,
            compaction_failures: 0,
            total_compactions: 2,
            auto_compactions: 1,
            manual_compactions: 1,
            last_compaction_breaker_reason: None,
            context_window_tokens: 128_000,
            compaction_threshold_tokens: 96_000,
            estimated_context_tokens: 64_000,
            message_count: 12,
            live_session_memory_initialized: true,
            live_session_memory_updating: false,
            live_session_memory_path: "/tmp/live.md".to_string(),
            session_tool_calls_total: 9,
            last_compaction_mode: None,
            last_compaction_at: None,
            last_compaction_summary_excerpt: None,
            last_compaction_session_memory_path: None,
            last_compaction_transcript_path: None,
            last_session_memory_update_at: None,
            last_session_memory_update_path: None,
            last_session_memory_generated_summary: false,
            session_memory_update_count: 3,
            tracked_failed_tool_results: 0,
            hook_total_executions: 0,
            hook_timeout_count: 0,
            hook_execution_error_count: 0,
            hook_nonzero_exit_count: 0,
            hook_wake_notification_count: 0,
            last_hook_failure_event: None,
            last_hook_failure_command: None,
            last_hook_failure_reason: None,
            last_hook_failure_at: None,
            last_hook_timeout_command: None,
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
            current_turn_tool_calls: 2,
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
            tool_progress_event_count: 4,
            last_tool_progress_message: None,
            last_tool_progress_tool: None,
            last_tool_progress_at: None,
            parallel_tool_batch_count: 1,
            parallel_tool_call_count: 3,
            max_parallel_batch_size: 2,
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
    fn runtime_health_checks_include_shared_runtime_summaries() {
        let dir = std::env::temp_dir().join(format!("yode-doctor-runtime-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".yode").join("transcripts")).unwrap();
        let rendered = runtime_health_checks(
            &dir,
            "session",
            &state(),
            &[],
            yode_core::PermissionMode::Default,
            &[],
            &["bash".to_string()],
            &[],
            "",
            &[],
        );
        assert!(rendered.iter().any(|line| line.contains("Runtime summary:")));
        assert!(rendered.iter().any(|line| line.contains("Context summary:")));
        assert!(rendered.iter().any(|line| line.contains("Tool summary:")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
