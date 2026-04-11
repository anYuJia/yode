use crate::commands::context::CommandContext;
use crate::commands::registry::VisibleCommandName;
use yode_core::updater::{latest_local_release_tag, release_version_matches_tag, CURRENT_VERSION};

pub(super) fn render_doctor_report(ctx: &mut CommandContext) -> String {
    let mut checks = Vec::new();
    let runtime = ctx.engine.try_lock().ok().map(|engine| {
        (
            engine.runtime_state(),
            engine.permissions().mode(),
            engine
                .permissions()
                .confirmable_tools()
                .into_iter()
                .map(|tool| tool.to_string())
                .collect::<Vec<_>>(),
        )
    });
    let project_root = std::path::PathBuf::from(&ctx.session.working_dir);

    if ctx.all_provider_models.is_empty() {
        checks.push(
            "  [!!] No LLM providers configured. Run /provider add to set one up.".to_string(),
        );
    } else {
        let names: Vec<_> = ctx.all_provider_models.keys().cloned().collect();
        checks.push(format!(
            "  [ok] LLM providers configured: {}",
            names.join(", ")
        ));
    }

    checks.push(
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
                checks.push(format!(
                    "  [ok] {} available: {}",
                    command,
                    String::from_utf8_lossy(&output.stdout).trim()
                ));
            }
            _ => checks.push(format!("  [--] {} not found (optional)", command)),
        }
    }

    checks.push(if ctx.terminal_caps.truecolor {
        "  [ok] Truecolor support enabled".to_string()
    } else {
        "  [--] No truecolor (using 256 colors)".to_string()
    });
    if ctx.terminal_caps.in_tmux {
        checks.push("  [--] Running inside tmux".to_string());
    }
    if ctx.terminal_caps.in_ssh {
        checks.push("  [--] Running over SSH".to_string());
    }

    let inventory = ctx.tools.inventory();
    checks.push(format!(
        "  [ok] tools: {} total / {} active / {} deferred",
        inventory.total_count, inventory.active_count, inventory.deferred_count
    ));
    checks.push(format!(
        "  [ok] mcp tools: {} active / {} deferred",
        inventory.mcp_active_count, inventory.mcp_deferred_count
    ));
    checks.push(format!(
        "  [ok] tool activations: {} (last: {})",
        inventory.activation_count,
        inventory.last_activated_tool.as_deref().unwrap_or("none")
    ));
    checks.push(format!(
        "  [ok] tool search: {} ({})",
        inventory.tool_search_enabled,
        inventory.tool_search_reason.as_deref().unwrap_or("no reason recorded")
    ));
    if inventory.duplicate_registration_count > 0 {
        checks.push(format!(
            "  [!!] Duplicate tool registrations blocked: {} ({})",
            inventory.duplicate_registration_count,
            inventory.duplicate_tool_names.join(", ")
        ));
    } else {
        checks.push("  [ok] No duplicate tool registrations observed".to_string());
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
        checks.push("  [ok] No command/tool naming overlaps detected".to_string());
    } else {
        checks.push(format!(
            "  [--] Command/tool naming overlaps: {}",
            command_tool_overlaps.join(", ")
        ));
    }
    if let Some(path) = dirs::home_dir().map(|home| home.join(".yode/config.toml")) {
        if path.exists() {
            checks.push(format!("  [ok] Config file: {:?}", path));
        } else {
            checks.push("  [!!] Config file missing".to_string());
        }
    }
    if let Some(profile) = ctx.session.startup_profile.as_deref() {
        checks.push(format!("  [ok] Startup profile: {}", profile));
    } else {
        checks.push("  [--] Startup profile unavailable".to_string());
    }

    if let Some((state, permission_mode, confirmable_tools)) = runtime {
        checks.extend(runtime_health_checks(
            &project_root,
            &state,
            permission_mode,
            &confirmable_tools,
        ));
    } else {
        checks.push("  [--] Engine runtime busy; skipped context/memory checks".to_string());
    }

    checks.push(match latest_local_release_tag() {
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
        "Yode Environment Health Check:\n\n{}\n\n  Platform: {} {}\n  Version:  v{}\n  Session:  {}",
        checks.join("\n"),
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
    state: &yode_core::engine::EngineRuntimeState,
    permission_mode: yode_core::PermissionMode,
    confirmable_tools: &[String],
) -> Vec<String> {
    let mut checks = Vec::new();
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
    checks.push(format!(
        "  [ok] Tool progress events tracked: {}",
        state.tool_progress_event_count
    ));
    checks.push(format!(
        "  [ok] Parallel tool batches tracked: {}",
        state.parallel_tool_batch_count
    ));

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
