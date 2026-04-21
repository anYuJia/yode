use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use crate::app::{
    format_scrollback_entry_as_strings, format_scrollback_grouped_system_batch,
    format_scrollback_grouped_tool_batch, ChatRole,
};
use crate::commands::artifact_nav::{
    artifact_freshness_badge, latest_coordinator_artifact,
    latest_runtime_orchestration_artifact, latest_workflow_execution_artifact,
};
use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};
use crate::runtime_artifacts::{
    write_runtime_task_inventory_artifact, write_runtime_timeline_artifact,
};
use crate::tool_grouping::{detect_groupable_system_batch, detect_groupable_tool_batch};
use crate::ui::status_summary::{
    context_window_summary_text, runtime_status_snapshot_from_parts,
    session_runtime_summary_text, tool_runtime_summary_text,
};

mod shared;
use shared::{
    dedup_artifact_paths, doctor_bundle_references, latest_artifact_candidates_from_links,
    latest_runtime_artifact_links, startup_artifact_candidates, truncate_preview_line,
};

pub struct ExportCommand {
    meta: CommandMeta,
}

impl ExportCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "export",
                description: "Export conversation or diagnostics bundle",
                aliases: &[],
                args: vec![ArgDef {
                    name: "target".to_string(),
                    required: false,
                    hint: "[file|diagnostics [name]]".to_string(),
                    completions: ArgCompletionSource::Static(vec!["diagnostics".to_string()]),
                }],
                category: CommandCategory::Utility,
                hidden: false,
            },
        }
    }
}

impl Command for ExportCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        if ctx.chat_entries.is_empty() {
            return Ok(CommandOutput::Message(
                "No conversation to export.".to_string(),
            ));
        }
        let parts = args.split_whitespace().collect::<Vec<_>>();
        if matches!(parts.as_slice(), ["diagnostics"] | ["diagnostics", ..]) {
            return export_diagnostics_bundle(parts.get(1).copied(), ctx);
        }

        // Generate default filename from first user message or timestamp
        let filename = if args.trim().is_empty() {
            let first_prompt = ctx
                .chat_entries
                .iter()
                .find(|e| matches!(e.role, ChatRole::User))
                .map(|e| {
                    let text = e.content.split('\n').next().unwrap_or("");
                    sanitize_filename(&truncate_preview_line(text, 80))
                })
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| timestamp_filename());

            format!("{}.txt", first_prompt)
        } else {
            let filename = args.trim();
            if filename.ends_with(".txt") {
                filename.to_string()
            } else {
                format!("{}.txt", filename)
            }
        };

        // Get current working directory
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let filepath = cwd.join(&filename);

        // Render conversation to text
        let content = render_conversation(ctx);

        // Write to file
        match File::create(&filepath) {
            Ok(mut file) => {
                if let Err(e) = file.write_all(content.as_bytes()) {
                    return Ok(CommandOutput::Message(format!(
                        "Failed to write file: {}",
                        e
                    )));
                }
                Ok(CommandOutput::Message(format!(
                    "Conversation exported to: {}",
                    filepath.display()
                )))
            }
            Err(e) => Ok(CommandOutput::Message(format!(
                "Failed to create file: {}",
                e
            ))),
        }
    }
}

/// Render conversation to plain text format
fn render_conversation(ctx: &CommandContext) -> String {
    let mut output = String::new();

    output.push_str("Conversation exported from Yode\n");
    output.push_str(&format!("Session: {}\n", ctx.session.session_id));
    output.push_str(&format!(
        "Date: {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    output.push_str(&format!("Model: {}\n\n", ctx.session.model));
    output.push_str(&"=".repeat(60));
    output.push_str("\n\n");
    output.push_str(&render_conversation_body(ctx.chat_entries));

    output.push_str(&"=".repeat(60));
    output.push_str("\n\n");
    output.push_str(&render_conversation_summary(ctx));

    output
}

fn render_conversation_body(entries: &[crate::app::ChatEntry]) -> String {
    let mut output = String::new();
    let mut index = 0;
    while index < entries.len() {
        let (lines, next_index) = if let Some(batch) = detect_groupable_tool_batch(entries, index) {
            (
                format_scrollback_grouped_tool_batch(entries, &batch),
                batch.next_index,
            )
        } else if let Some(batch) = detect_groupable_system_batch(entries, index) {
            (
                format_scrollback_grouped_system_batch(entries, &batch),
                batch.next_index,
            )
        } else {
            (
                format_scrollback_entry_as_strings(&entries[index], entries, index),
                index + 1,
            )
        };

        for (line, _) in lines {
            output.push_str(&line);
            output.push('\n');
        }
        if next_index < entries.len() {
            output.push('\n');
        }
        index = next_index;
    }
    output
}

fn export_diagnostics_bundle(custom_name: Option<&str>, ctx: &mut CommandContext) -> CommandResult {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let bundle_name = custom_name
        .map(sanitize_filename)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("diagnostics-{}", timestamp_filename()));
    let bundle_dir = cwd.join(bundle_name);
    std::fs::create_dir_all(&bundle_dir)
        .map_err(|err| format!("Failed to create diagnostics bundle dir: {}", err))?;

    let conversation_path = bundle_dir.join("conversation.txt");
    std::fs::write(&conversation_path, render_conversation(ctx))
        .map_err(|err| format!("Failed to write {}: {}", conversation_path.display(), err))?;

    let diagnostics_path = bundle_dir.join("runtime-summary.txt");
    let (runtime, tasks) = ctx
        .engine
        .try_lock()
        .ok()
        .map(|engine| (Some(engine.runtime_state()), engine.runtime_tasks_snapshot()))
        .unwrap_or((None, Vec::new()));
    let project_root = PathBuf::from(&ctx.session.working_dir);
    let runtime_summary = render_runtime_bundle_summary(&project_root, runtime.as_ref(), &tasks);
    std::fs::write(&diagnostics_path, runtime_summary)
        .map_err(|err| format!("Failed to write {}: {}", diagnostics_path.display(), err))?;

    let timeline_path = bundle_dir.join("runtime-timeline.txt");
    let timeline_body = if let Some(state) = runtime.as_ref() {
        write_runtime_timeline_artifact(
            &PathBuf::from(&ctx.session.working_dir),
            &ctx.session.session_id,
            state,
            &tasks,
        )
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_else(|| "Runtime timeline unavailable: artifact write failed.\n".to_string())
    } else {
        "Runtime timeline unavailable: engine busy.\n".to_string()
    };
    std::fs::write(&timeline_path, timeline_body)
        .map_err(|err| format!("Failed to write {}: {}", timeline_path.display(), err))?;

    let mut copied = Vec::new();
    for path in latest_artifact_candidates(ctx) {
        if path.exists() {
            let dest = bundle_dir.join(
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("artifact.txt"),
            );
            if std::fs::copy(&path, &dest).is_ok() {
                copied.push(dest.display().to_string());
            }
        }
    }

    let doctor_refs = doctor_bundle_references(&cwd);
    let doctor_ref_path = bundle_dir.join("doctor-bundles.txt");
    if !doctor_refs.is_empty() {
        let content = doctor_refs
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let _ = std::fs::write(&doctor_ref_path, content);
    }

    let workspace_index = bundle_dir.join("workspace-index.md");
    let workflow_artifact = latest_workflow_execution_artifact(&project_root)
        .map(|path| format!("[{}] {}", artifact_freshness_badge(&path), path.display()))
        .unwrap_or_else(|| "none".to_string());
    let coordinator_artifact = latest_coordinator_artifact(&project_root)
        .map(|path| format!("[{}] {}", artifact_freshness_badge(&path), path.display()))
        .unwrap_or_else(|| "none".to_string());
    let orchestration_artifact = latest_runtime_orchestration_artifact(&project_root)
        .map(|path| format!("[{}] {}", artifact_freshness_badge(&path), path.display()))
        .unwrap_or_else(|| "none".to_string());
    let workspace_body = render_workspace_index(
        &bundle_dir,
        &project_root,
        runtime.as_ref(),
        &tasks,
        &conversation_path,
        &diagnostics_path,
        &timeline_path,
        if doctor_refs.is_empty() {
            None
        } else {
            Some(doctor_ref_path.as_path())
        },
        &workflow_artifact,
        &coordinator_artifact,
        &orchestration_artifact,
    );
    let _ = std::fs::write(&workspace_index, workspace_body);

    let runtime_snapshot = runtime.as_ref().map(|state| {
        runtime_status_snapshot_from_parts(
            &project_root,
            Some(state.clone()),
            tasks.iter()
                .filter(|task| matches!(task.status, yode_tools::RuntimeTaskStatus::Running))
                .count(),
        )
    });
    let runtime_line = runtime_snapshot
        .as_ref()
        .map(|snapshot| session_runtime_summary_text(snapshot, runtime.as_ref().map(|state| state.estimated_context_tokens).unwrap_or(0)))
        .unwrap_or_else(|| "engine busy".to_string());
    let context_line = runtime
        .as_ref()
        .map(|state| context_window_summary_text(Some(state), state.estimated_context_tokens))
        .unwrap_or_else(|| "engine busy".to_string());
    let tool_line = runtime
        .as_ref()
        .map(tool_runtime_summary_text)
        .unwrap_or_else(|| "engine busy".to_string());

    Ok(CommandOutput::Message(render_bundle_completion_message(
        &bundle_dir,
        &runtime_line,
        &context_line,
        &tool_line,
        &conversation_path,
        &diagnostics_path,
        &workspace_index,
        &copied,
        if doctor_refs.is_empty() {
            None
        } else {
            Some(doctor_ref_path.as_path())
        },
    )))
}

fn latest_artifact_candidates(ctx: &mut CommandContext) -> Vec<PathBuf> {
    let runtime = ctx
        .engine
        .try_lock()
        .ok()
        .map(|engine| engine.runtime_state());
    let project_root = PathBuf::from(&ctx.session.working_dir);
    let mut paths =
        latest_artifact_candidates_from_links(&latest_runtime_artifact_links(runtime.clone()));
    let runtime_tasks = ctx
        .engine
        .try_lock()
        .ok()
        .map(|engine| engine.runtime_tasks_snapshot())
        .unwrap_or_default();
    if let Some(runtime_task_artifact) = write_runtime_task_inventory_artifact(
        &project_root,
        &ctx.session.session_id,
        runtime.as_ref(),
        runtime_tasks,
    ) {
        paths.push(PathBuf::from(runtime_task_artifact));
    }
    if let Some(path) = latest_workflow_execution_artifact(&project_root) {
        paths.push(path);
    }
    if let Some(path) = latest_coordinator_artifact(&project_root) {
        paths.push(path);
    }
    if let Some(path) = latest_runtime_orchestration_artifact(&project_root) {
        paths.push(path);
    }
    paths.extend(startup_artifact_candidates(&project_root));

    let review_dir = PathBuf::from(&ctx.session.working_dir)
        .join(".yode")
        .join("reviews");
    let mut reviews = std::fs::read_dir(&review_dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .collect::<Vec<_>>();
    reviews.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    if let Some(latest_review) = reviews.into_iter().next() {
        paths.push(latest_review);
    }
    dedup_artifact_paths(paths)
}

fn render_runtime_bundle_summary(
    project_root: &std::path::Path,
    runtime: Option<&yode_core::engine::EngineRuntimeState>,
    tasks: &[yode_tools::RuntimeTask],
) -> String {
    let running_tasks = tasks
        .iter()
        .filter(|task| matches!(task.status, yode_tools::RuntimeTaskStatus::Running))
        .count();
    let Some(state) = runtime else {
        return "Runtime summary\n\n- Runtime: engine busy\n- Context: engine busy\n- Tools: engine busy\n- Tasks: unavailable\n".to_string();
    };
    let snapshot =
        runtime_status_snapshot_from_parts(project_root, Some(state.clone()), running_tasks);
    format!(
        "Runtime summary\n\n- Runtime: {}\n- Context: {}\n- Tools: {}\n- Tasks: total {} / running {}\n- Tool artifact: {}\n- Transcript: {}\n- Compact summary: {}\n- Prompt cache turns: {}\n- System prompt est: {} tokens\n",
        session_runtime_summary_text(&snapshot, state.estimated_context_tokens),
        context_window_summary_text(Some(state), state.estimated_context_tokens),
        tool_runtime_summary_text(state),
        tasks.len(),
        running_tasks,
        state.last_tool_turn_artifact_path.as_deref().unwrap_or("none"),
        state.last_compaction_transcript_path.as_deref().unwrap_or("none"),
        state.last_compaction_summary_excerpt.as_deref().unwrap_or("none"),
        state.prompt_cache.reported_turns,
        state.system_prompt_estimated_tokens,
    )
}

fn render_workspace_index(
    bundle_dir: &std::path::Path,
    project_root: &std::path::Path,
    runtime: Option<&yode_core::engine::EngineRuntimeState>,
    tasks: &[yode_tools::RuntimeTask],
    conversation_path: &std::path::Path,
    diagnostics_path: &std::path::Path,
    timeline_path: &std::path::Path,
    doctor_ref_path: Option<&std::path::Path>,
    workflow_artifact: &str,
    coordinator_artifact: &str,
    orchestration_artifact: &str,
) -> String {
    let running_tasks = tasks
        .iter()
        .filter(|task| matches!(task.status, yode_tools::RuntimeTaskStatus::Running))
        .count();
    let (runtime_line, context_line, tool_line) = if let Some(state) = runtime {
        let snapshot =
            runtime_status_snapshot_from_parts(project_root, Some(state.clone()), running_tasks);
        (
            session_runtime_summary_text(&snapshot, state.estimated_context_tokens),
            context_window_summary_text(Some(state), state.estimated_context_tokens),
            tool_runtime_summary_text(state),
        )
    } else {
        (
            "engine busy".to_string(),
            "engine busy".to_string(),
            "engine busy".to_string(),
        )
    };
    format!(
        "# Workspace Index\n\n## Summary\n\n- Bundle: {}\n- Runtime: {}\n- Context: {}\n- Tools: {}\n- Tasks: total {} / running {}\n- Conversation: {}\n- Runtime summary: {}\n- Runtime timeline: {}\n- Doctor refs: {}\n\n## Jump Targets\n\n- /tasks latest\n- /memory latest\n- /reviews latest\n- /status\n- /diagnostics\n- /doctor bundle\n\n## Orchestration Artifacts\n\n- workflow: {}\n- coordinator: {}\n- timeline: {}\n\n## Inspect Aliases\n\n- /inspect artifact summary\n- /inspect artifact latest-workflow\n- /inspect artifact latest-coordinate\n- /inspect artifact latest-orchestration\n- /inspect artifact latest-runtime-timeline\n- /inspect artifact latest-provider-inventory\n- /inspect artifact latest-review\n- /inspect artifact latest-transcript\n- /inspect artifact bundle\n",
        bundle_dir.display(),
        runtime_line,
        context_line,
        tool_line,
        tasks.len(),
        running_tasks,
        conversation_path.display(),
        diagnostics_path.display(),
        timeline_path.display(),
        doctor_ref_path
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string()),
        workflow_artifact,
        coordinator_artifact,
        orchestration_artifact,
    )
}

fn render_conversation_summary(ctx: &CommandContext) -> String {
    let project_root = PathBuf::from(&ctx.session.working_dir);
    let (runtime, tasks) = ctx
        .engine
        .try_lock()
        .ok()
        .map(|engine| (Some(engine.runtime_state()), engine.runtime_tasks_snapshot()))
        .unwrap_or((None, Vec::new()));
    let running_tasks = tasks
        .iter()
        .filter(|task| matches!(task.status, yode_tools::RuntimeTaskStatus::Running))
        .count();

    let (runtime_line, context_line, tool_line) = if let Some(state) = runtime.as_ref() {
        let snapshot =
            runtime_status_snapshot_from_parts(&project_root, Some(state.clone()), running_tasks);
        (
            session_runtime_summary_text(&snapshot, state.estimated_context_tokens),
            context_window_summary_text(Some(state), state.estimated_context_tokens),
            tool_runtime_summary_text(state),
        )
    } else {
        (
            "engine busy".to_string(),
            "engine busy".to_string(),
            "engine busy".to_string(),
        )
    };

    render_conversation_summary_block(
        &runtime_line,
        &context_line,
        &tool_line,
        ctx.chat_entries.len(),
        ctx.session.input_tokens,
        ctx.session.output_tokens,
        ctx.session.total_tokens,
        ctx.session.tool_call_count,
        tasks.len(),
        running_tasks,
    )
}

fn render_conversation_summary_block(
    runtime_line: &str,
    context_line: &str,
    tool_line: &str,
    entry_count: usize,
    input_tokens: u32,
    output_tokens: u32,
    total_tokens: u32,
    tool_calls: u32,
    total_tasks: usize,
    running_tasks: usize,
) -> String {
    format!(
        "Summary:\n  Runtime:      {}\n  Context:      {}\n  Tools:        {}\n  Entries:       {}\n  Tokens:        in {} / out {} / total {}\n  Tool calls:    {}\n  Tasks:         total {} / running {}\n",
        runtime_line,
        context_line,
        tool_line,
        entry_count,
        input_tokens,
        output_tokens,
        total_tokens,
        tool_calls,
        total_tasks,
        running_tasks,
    )
}

fn render_bundle_completion_message(
    bundle_dir: &std::path::Path,
    runtime_line: &str,
    context_line: &str,
    tool_line: &str,
    conversation_path: &std::path::Path,
    diagnostics_path: &std::path::Path,
    workspace_index: &std::path::Path,
    copied: &[String],
    doctor_ref_path: Option<&std::path::Path>,
) -> String {
    format!(
        "Diagnostics bundle exported to: {}\n  Runtime:      {}\n  Context:      {}\n  Tools:        {}\n  Core files:    {}, {}, {}\n  Copied files:  {}\n  Doctor refs:   {}\n  Inspect:       /inspect artifact bundle",
        bundle_dir.display(),
        runtime_line,
        context_line,
        tool_line,
        conversation_path.display(),
        diagnostics_path.display(),
        workspace_index.display(),
        if copied.is_empty() {
            "none".to_string()
        } else {
            format!("{}", copied.len())
        },
        doctor_ref_path
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string()),
    )
}

/// Sanitize string for use as filename
fn sanitize_filename(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join("-")
        .chars()
        .take(50)
        .collect()
}

/// Generate timestamp-based filename
fn timestamp_filename() -> String {
    chrono::Local::now().format("%Y-%m-%d-%H%M%S").to_string()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        render_bundle_completion_message, render_conversation_body, render_conversation_summary_block,
        render_runtime_bundle_summary, render_workspace_index,
    };
    use crate::app::{ChatEntry, ChatRole};
    use yode_core::engine::{EngineRuntimeState, PromptCacheRuntimeState};
    use yode_core::tool_runtime::ToolRuntimeCallView;
    use yode_tools::registry::ToolPoolSnapshot;
    use yode_tools::RuntimeTask;

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
            message_count: 10,
            live_session_memory_initialized: true,
            live_session_memory_updating: false,
            live_session_memory_path: String::new(),
            session_tool_calls_total: 6,
            last_compaction_mode: None,
            last_compaction_at: None,
            last_compaction_summary_excerpt: Some("trimmed old turns".to_string()),
            last_compaction_session_memory_path: None,
            last_compaction_transcript_path: Some("/tmp/transcript.md".to_string()),
            last_session_memory_update_at: None,
            last_session_memory_update_path: None,
            last_session_memory_generated_summary: false,
            session_memory_update_count: 2,
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
            system_prompt_estimated_tokens: 1234,
            system_prompt_segments: Vec::new(),
            prompt_cache: PromptCacheRuntimeState {
                reported_turns: 4,
                ..PromptCacheRuntimeState::default()
            },
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
            current_turn_tool_calls: 1,
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
            tool_progress_event_count: 3,
            last_tool_progress_message: None,
            last_tool_progress_tool: None,
            last_tool_progress_at: None,
            parallel_tool_batch_count: 1,
            parallel_tool_call_count: 2,
            max_parallel_batch_size: 2,
            tool_truncation_count: 0,
            last_tool_truncation_reason: None,
            latest_repeated_tool_failure: None,
            read_file_history: Vec::new(),
            command_tool_duplication_hints: Vec::new(),
            last_tool_turn_completed_at: None,
            last_tool_turn_artifact_path: Some("/tmp/tool.md".to_string()),
            tool_error_type_counts: BTreeMap::new(),
            tool_trace_scope: "last".to_string(),
            tool_traces: Vec::<ToolRuntimeCallView>::new(),
        }
    }

    #[test]
    fn runtime_bundle_summary_uses_shared_runtime_lines() {
        let rendered = render_runtime_bundle_summary(std::path::Path::new("/tmp"), Some(&state()), &[]);
        assert!(rendered.contains("- Runtime:"));
        assert!(rendered.contains("- Context:"));
        assert!(rendered.contains("- Tools:"));
        assert!(rendered.contains("- Prompt cache turns: 4"));
    }

    #[test]
    fn workspace_index_includes_summary_section() {
        let rendered = render_workspace_index(
            std::path::Path::new("/tmp/bundle"),
            std::path::Path::new("/tmp"),
            Some(&state()),
            &Vec::<RuntimeTask>::new(),
            std::path::Path::new("/tmp/bundle/conversation.txt"),
            std::path::Path::new("/tmp/bundle/runtime-summary.txt"),
            std::path::Path::new("/tmp/bundle/runtime-timeline.txt"),
            None,
            "workflow",
            "coordinate",
            "timeline",
        );
        assert!(rendered.contains("## Summary"));
        assert!(rendered.contains("- Runtime:"));
        assert!(rendered.contains("## Jump Targets"));
        assert!(rendered.contains("## Inspect Aliases"));
    }

    #[test]
    fn bundle_completion_message_uses_summary_layout() {
        let rendered = render_bundle_completion_message(
            std::path::Path::new("/tmp/bundle"),
            "runtime",
            "context",
            "tools",
            std::path::Path::new("/tmp/bundle/conversation.txt"),
            std::path::Path::new("/tmp/bundle/runtime-summary.txt"),
            std::path::Path::new("/tmp/bundle/workspace-index.md"),
            &["a".to_string(), "b".to_string()],
            None,
        );
        assert!(rendered.contains("Diagnostics bundle exported to:"));
        assert!(rendered.contains("Core files:"));
        assert!(rendered.contains("Copied files:  2"));
    }

    #[test]
    fn conversation_summary_block_renders_shared_lines() {
        let summary = render_conversation_summary_block(
            "runtime",
            "context",
            "tools",
            3,
            10,
            20,
            30,
            2,
            4,
            1,
        );
        assert!(summary.contains("Summary:"));
        assert!(summary.contains("Runtime:      runtime"));
        assert!(summary.contains("Context:      context"));
        assert!(summary.contains("Tools:        tools"));
        assert!(summary.contains("Entries:       3"));
        assert!(summary.contains("Tool calls:    2"));
    }

    #[test]
    fn conversation_body_uses_grouped_and_styled_export_rendering() {
        let mut entries = vec![
            ChatEntry::new(ChatRole::User, "show me the code".to_string()),
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "grep".to_string(),
                },
                "{\"pattern\":\"retry\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "a".to_string(),
                    name: "grep".to_string(),
                    is_error: false,
                },
                "src/app.rs:12: retry".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "b".to_string(),
                    name: "read_file".to_string(),
                },
                "{\"file_path\":\"/tmp/src/app.rs\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "b".to_string(),
                    name: "read_file".to_string(),
                    is_error: false,
                },
                "fn retry() {}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::System,
                "Turn completed · 1.4s · 2 tools · 1.2k↑ 180↓ tok".to_string(),
            ),
        ];
        entries[2].tool_metadata = Some(serde_json::json!({
            "output_mode": "content",
            "line_count": 1,
            "file_count": 1,
            "match_count": 1,
            "pattern": "retry"
        }));
        entries[4].tool_metadata = Some(serde_json::json!({
            "file_path": "/tmp/src/app.rs",
            "total_lines": 40,
            "start_line": 1,
            "end_line": 20,
            "was_truncated": true
        }));

        let body = render_conversation_body(&entries);
        assert!(body.contains("⏺ Searched for 1 pattern, read 1 file"));
        assert!(body.contains("Turn completed"));
        assert!(!body.contains("--- [Tool: grep]"));
    }
}
