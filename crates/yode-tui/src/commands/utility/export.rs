use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::app::rendering::strip_ansi;
use crate::app::{
    format_scrollback_entry_as_strings, format_scrollback_grouped_subagent_batch,
    format_scrollback_grouped_system_batch, format_scrollback_grouped_tool_batch, ChatRole,
};
use crate::commands::artifact_nav::{
    artifact_freshness_badge, export_bundle_root, latest_coordinator_artifact,
    latest_mcp_resource_artifact, latest_media_compact_events_artifact,
    latest_post_compact_restore_artifact, latest_post_compact_restore_diff_artifact,
    latest_post_compact_restore_state_artifact, latest_prompt_cache_artifact,
    latest_prompt_cache_break_artifact, latest_prompt_cache_diff_artifact,
    latest_prompt_cache_events_artifact, latest_prompt_cache_state_artifact,
    latest_runtime_orchestration_artifact, latest_workflow_execution_artifact,
};
use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};
use crate::display_text::compact_path_tail;
use crate::mcp_resource_artifacts::mcp_resource_manifest_summary;
use crate::runtime_artifacts::{
    write_prompt_cache_artifact, write_runtime_task_inventory_artifact,
    write_runtime_timeline_artifact,
};
use crate::tool_grouping::{
    detect_groupable_subagent_batch, detect_groupable_system_batch, detect_groupable_tool_batch,
    should_hide_tool_from_transcript,
};
use crate::ui::status_summary::{
    context_window_summary_text, runtime_status_snapshot_from_parts, session_runtime_summary_text,
    tool_runtime_summary_text,
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
                .unwrap_or_else(timestamp_filename);

            format!("{}.txt", first_prompt)
        } else {
            let filename = args.trim();
            if filename.ends_with(".txt") {
                filename.to_string()
            } else {
                format!("{}.txt", filename)
            }
        };

        let project_root = PathBuf::from(&ctx.session.working_dir);
        let filepath = conversation_export_path(&project_root, &filename).map_err(|err| {
            format!(
                "Failed to prepare conversation export path for {}: {}",
                filename, err
            )
        })?;

        // Render conversation to text
        let content = render_conversation(ctx);

        // Write to file
        match File::create(&filepath) {
            Ok(mut file) => {
                if let Err(e) = file.write_all(content.as_bytes()) {
                    return Ok(CommandOutput::Message(format!(
                        "Failed to write {}: {}",
                        compact_path_tail(&filepath.display().to_string()),
                        e,
                    )));
                }
                Ok(CommandOutput::Message(format!(
                    "Conversation export written: {}",
                    compact_path_tail(&filepath.display().to_string())
                )))
            }
            Err(e) => Ok(CommandOutput::Message(format!(
                "Failed to create {}: {}",
                compact_path_tail(&filepath.display().to_string()),
                e,
            ))),
        }
    }
}

fn conversation_export_path(
    project_root: &std::path::Path,
    filename: &str,
) -> std::io::Result<PathBuf> {
    let export_root = export_bundle_root(project_root);
    std::fs::create_dir_all(&export_root)?;
    Ok(export_root.join(filename))
}

/// Render conversation to plain text format
fn render_conversation(ctx: &CommandContext) -> String {
    let mut output = String::new();

    output.push_str("# Conversation Export\n\n");
    output.push_str(&format!("- Session: {}\n", ctx.session.session_id));
    output.push_str(&format!(
        "- Date: {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    output.push_str(&format!("- Model: {}\n\n", ctx.session.model));
    output.push_str("## Transcript\n\n");
    output.push_str(&render_conversation_body(ctx.chat_entries));
    output.push('\n');
    output.push_str("## Session Summary\n\n");
    output.push_str(&render_conversation_summary(ctx));

    output
}

fn render_conversation_body(entries: &[crate::app::ChatEntry]) -> String {
    let mut output = String::new();
    let mut index = 0;
    while index < entries.len() {
        match &entries[index].role {
            ChatRole::ToolCall { name, .. } | ChatRole::ToolResult { name, .. }
                if should_hide_tool_from_transcript(name) =>
            {
                index += 1;
                continue;
            }
            _ => {}
        }
        let (lines, next_index) =
            if let Some(batch) = detect_groupable_subagent_batch(entries, index) {
                (
                    format_scrollback_grouped_subagent_batch(entries, &batch),
                    batch.next_index,
                )
            } else if let Some(batch) = detect_groupable_tool_batch(entries, index) {
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

        if let Some(heading) = conversation_block_heading(entries, index, next_index) {
            output.push_str(&format!("### {}\n\n", heading));
        }

        for (line, _) in lines {
            output.push_str(&strip_ansi(&line));
            output.push('\n');
        }
        if next_index < entries.len() {
            output.push('\n');
        }
        index = next_index;
    }
    output
}

fn conversation_block_heading(
    entries: &[crate::app::ChatEntry],
    index: usize,
    next_index: usize,
) -> Option<&'static str> {
    let entry = entries.get(index)?;
    if next_index > index + 1 {
        return match entry.role {
            ChatRole::ToolCall { .. } => Some("Tool Activity"),
            ChatRole::System => Some("System"),
            ChatRole::SubAgentCall { .. } => Some("Subagent Activity"),
            _ => None,
        };
    }

    match entry.role {
        ChatRole::User => Some("User"),
        ChatRole::Assistant => Some("Assistant"),
        ChatRole::ToolCall { .. } | ChatRole::ToolResult { .. } => Some("Tool Activity"),
        ChatRole::System => Some("System"),
        ChatRole::Error => Some("Error"),
        ChatRole::SubAgentCall { .. }
        | ChatRole::SubAgentToolCall { .. }
        | ChatRole::SubAgentResult => Some("Subagent Activity"),
        ChatRole::AskUser { .. } => Some("Ask User"),
    }
}

fn export_diagnostics_bundle(custom_name: Option<&str>, ctx: &mut CommandContext) -> CommandResult {
    let project_root = PathBuf::from(&ctx.session.working_dir);
    let bundle_root = export_bundle_root(&project_root);
    std::fs::create_dir_all(&bundle_root)
        .map_err(|err| format!("Failed to create diagnostics export dir: {}", err))?;
    let bundle_name = custom_name
        .map(sanitize_filename)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("diagnostics-{}", timestamp_filename()));
    let bundle_dir = bundle_root.join(bundle_name);
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
        .map(|engine| {
            (
                Some(engine.runtime_state()),
                engine.runtime_tasks_snapshot(),
            )
        })
        .unwrap_or((None, Vec::new()));
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

    let prompt_cache_path = bundle_dir.join("prompt-cache.txt");
    let prompt_cache_body = if let Some(state) = runtime.as_ref() {
        write_prompt_cache_artifact(
            &PathBuf::from(&ctx.session.working_dir),
            &ctx.session.session_id,
            state,
        )
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_else(|| "Prompt cache artifact unavailable.\n".to_string())
    } else {
        "Prompt cache artifact unavailable: engine busy.\n".to_string()
    };
    std::fs::write(&prompt_cache_path, prompt_cache_body)
        .map_err(|err| format!("Failed to write {}: {}", prompt_cache_path.display(), err))?;

    let artifact_candidates = latest_artifact_candidates(ctx);
    let mcp_resource_candidates = artifact_candidates
        .iter()
        .filter(|path| is_mcp_resource_artifact(path))
        .cloned()
        .collect::<Vec<_>>();
    let mut copied = Vec::new();
    for path in artifact_candidates {
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
    if let Some(index_path) = write_mcp_resource_export_index(&bundle_dir, &mcp_resource_candidates)
    {
        copied.push(index_path.display().to_string());
    }

    let doctor_refs = doctor_bundle_references(&project_root);
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
        &prompt_cache_path,
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
            tasks
                .iter()
                .filter(|task| matches!(task.status, yode_tools::RuntimeTaskStatus::Running))
                .count(),
        )
    });
    let runtime_line = runtime_snapshot
        .as_ref()
        .map(|snapshot| {
            session_runtime_summary_text(
                snapshot,
                runtime
                    .as_ref()
                    .map(|state| state.estimated_context_tokens)
                    .unwrap_or(0),
            )
        })
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
        &prompt_cache_path,
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
    let mut paths = latest_artifact_candidates_from_links(&latest_runtime_artifact_links(
        &project_root,
        runtime.clone(),
    ));
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
    if let Some(path) = latest_prompt_cache_artifact(&project_root) {
        paths.push(path);
    }
    if let Some(path) = latest_prompt_cache_state_artifact(&project_root) {
        paths.push(path);
    }
    if let Some(path) = latest_prompt_cache_events_artifact(&project_root) {
        paths.push(path);
    }
    if let Some(path) = latest_media_compact_events_artifact(&project_root) {
        paths.push(path);
    }
    if let Some(path) = latest_prompt_cache_break_artifact(&project_root) {
        paths.push(path);
    }
    if let Some(path) = latest_prompt_cache_diff_artifact(&project_root) {
        paths.push(path);
    }
    if let Some(path) = latest_post_compact_restore_artifact(&project_root) {
        paths.push(path);
    }
    if let Some(path) = latest_post_compact_restore_state_artifact(&project_root) {
        paths.push(path);
    }
    if let Some(path) = latest_post_compact_restore_diff_artifact(&project_root) {
        paths.push(path);
    }
    paths.extend(mcp_resource_artifact_candidates(&project_root, 12));
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

fn mcp_resource_artifact_candidates(project_root: &std::path::Path, limit: usize) -> Vec<PathBuf> {
    let dir = project_root
        .join(".yode")
        .join("status")
        .join("mcp-resources");
    let mut paths = std::fs::read_dir(&dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    paths.sort_by(|left, right| {
        let left_modified = left
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let right_modified = right
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        right_modified
            .cmp(&left_modified)
            .then_with(|| right.file_name().cmp(&left.file_name()))
    });
    paths.into_iter().take(limit).collect()
}

fn is_mcp_resource_artifact(path: &std::path::Path) -> bool {
    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        == Some("mcp-resources")
}

fn write_mcp_resource_export_index(
    bundle_dir: &std::path::Path,
    candidates: &[PathBuf],
) -> Option<PathBuf> {
    if candidates.is_empty() {
        return None;
    }
    let index_path = bundle_dir.join("mcp-resources-index.md");
    std::fs::write(
        &index_path,
        crate::mcp_resource_artifacts::render_mcp_resource_artifact_index(candidates),
    )
    .ok()?;
    Some(index_path)
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
    prompt_cache_path: &std::path::Path,
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
    let mcp_resource_artifact = latest_mcp_resource_artifact(project_root)
        .map(|path| mcp_resource_artifact_index_line(&path))
        .unwrap_or_else(|| "none".to_string());
    let mcp_resource_index = bundle_dir.join("mcp-resources-index.md");
    let mcp_resource_index = if mcp_resource_index.exists() {
        mcp_resource_index.display().to_string()
    } else {
        "none".to_string()
    };
    format!(
        "# Workspace Index\n\n## Summary\n\n- Bundle: {}\n- Runtime: {}\n- Context: {}\n- Tools: {}\n- Tasks: total {} / running {}\n- Conversation: {}\n- Runtime summary: {}\n- Runtime timeline: {}\n- Prompt cache: {}\n- Doctor refs: {}\n\n## Jump\n\n- Status: /status · /diagnostics · /doctor bundle\n- Work: /tasks latest · /memory latest · /reviews latest\n\n## Orchestration\n\n- workflow: {}\n- coordinator: {}\n- timeline: {}\n\n## MCP\n\n- resource: {}\n- resource index: {}\n\n## Inspect\n\n- Overview: /inspect artifact summary · bundle\n- Flow: /inspect artifact latest-workflow · latest-coordinate · latest-orchestration\n- Runtime: /inspect artifact latest-runtime-timeline · latest-prompt-cache · latest-prompt-cache-state\n- Cache: /inspect artifact latest-prompt-cache-events · latest-media-compact-events · latest-prompt-cache-break · latest-prompt-cache-diff\n- Restore: /inspect artifact latest-post-compact-restore · latest-post-compact-restore-state · latest-post-compact-restore-diff\n- MCP: /inspect artifact latest-mcp-resource · latest-mcp-resource-index · history mcp-resources\n- Refs: /inspect artifact latest-provider-inventory · latest-review · latest-transcript\n",
        compact_path_tail(&bundle_dir.display().to_string()),
        runtime_line,
        context_line,
        tool_line,
        tasks.len(),
        running_tasks,
        conversation_path.display(),
        diagnostics_path.display(),
        timeline_path.display(),
        prompt_cache_path.display(),
        doctor_ref_path
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string()),
        workflow_artifact,
        coordinator_artifact,
        orchestration_artifact,
        mcp_resource_artifact,
        mcp_resource_index,
    )
}

fn mcp_resource_artifact_index_line(path: &Path) -> String {
    let base = format!("[{}] {}", artifact_freshness_badge(path), path.display());
    let Some(summary) = mcp_resource_manifest_summary(path, false, " · ") else {
        return base;
    };
    format!("{} · {}", base, summary)
}

fn render_conversation_summary(ctx: &CommandContext) -> String {
    let project_root = PathBuf::from(&ctx.session.working_dir);
    let (runtime, tasks) = ctx
        .engine
        .try_lock()
        .ok()
        .map(|engine| {
            (
                Some(engine.runtime_state()),
                engine.runtime_tasks_snapshot(),
            )
        })
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
        "- Runtime: {}\n- Context: {}\n- Tools: {}\n- Entries: {}\n- Tokens: in {} / out {} / total {}\n- Tool calls: {}\n- Tasks: total {} / running {}\n",
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
    prompt_cache_path: &std::path::Path,
    copied: &[String],
    doctor_ref_path: Option<&std::path::Path>,
) -> String {
    let core = [
        conversation_path,
        diagnostics_path,
        workspace_index,
        prompt_cache_path,
    ]
    .iter()
    .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
    .collect::<Vec<_>>()
    .join(", ");
    format!(
        "Diagnostics bundle written: {}\n  Runtime: {}\n  Context: {}\n  Tools: {}\n  Core: {}\n  Extras: {} copied · doctor {}\n  Inspect: /inspect artifact bundle · /diagnostics",
        compact_path_tail(&bundle_dir.display().to_string()),
        runtime_line,
        context_line,
        tool_line,
        core,
        if copied.is_empty() {
            "none".to_string()
        } else {
            copied.len().to_string()
        },
        doctor_ref_path
            .and_then(|path| path.file_name().and_then(|name| name.to_str()))
            .unwrap_or("none")
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
        conversation_export_path, is_mcp_resource_artifact, mcp_resource_artifact_candidates,
        render_bundle_completion_message, render_conversation_body,
        render_conversation_summary_block, render_runtime_bundle_summary, render_workspace_index,
        write_mcp_resource_export_index,
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
            last_compact_boundary: None,
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
            avg_compaction_prompt_tokens: None,
            compaction_cause_histogram: BTreeMap::new(),
            last_microcompact_media_removed: 0,
            last_microcompact_media_saved_chars: 0,
            microcompact_media_removed_total: 0,
            microcompact_media_saved_chars_total: 0,
            system_prompt_estimated_tokens: 1234,
            system_prompt_segments: Vec::new(),
            prompt_cache: PromptCacheRuntimeState {
                reported_turns: 4,
                ..PromptCacheRuntimeState::default()
            },
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
        let rendered =
            render_runtime_bundle_summary(std::path::Path::new("/tmp"), Some(&state()), &[]);
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
            std::path::Path::new("/tmp/bundle/prompt-cache.txt"),
            None,
            "workflow",
            "coordinate",
            "timeline",
        );
        assert!(rendered.contains("## Summary"));
        assert!(rendered.contains("- Bundle: .../tmp/bundle"));
        assert!(rendered.contains("- Runtime:"));
        assert!(rendered.contains("## Jump"));
        assert!(rendered.contains("## MCP"));
        assert!(rendered.contains("## Inspect"));
        assert!(rendered.contains("- Status: /status · /diagnostics · /doctor bundle"));
        assert!(rendered.contains("Work: /tasks latest · /memory latest · /reviews latest"));
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
            std::path::Path::new("/tmp/bundle/prompt-cache.txt"),
            &["a".to_string(), "b".to_string()],
            None,
        );
        assert!(rendered.contains("Diagnostics bundle written:"));
        assert!(rendered.contains(
            "Core: conversation.txt, runtime-summary.txt, workspace-index.md, prompt-cache.txt"
        ));
        assert!(rendered.contains("Extras: 2 copied · doctor none"));
        assert!(rendered.contains("/diagnostics"));
    }

    #[test]
    fn conversation_export_path_uses_workspace_export_root() {
        let dir = std::env::temp_dir().join(format!("yode-export-path-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let path = conversation_export_path(&dir, "demo.txt").unwrap();
        assert!(path.ends_with(".yode/exports/demo.txt"));
        assert!(path.parent().is_some_and(|parent| parent.exists()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn conversation_summary_block_renders_shared_lines() {
        let summary = render_conversation_summary_block(
            "runtime", "context", "tools", 3, 10, 20, 30, 2, 4, 1,
        );
        assert!(summary.contains("- Runtime: runtime"));
        assert!(summary.contains("- Context: context"));
        assert!(summary.contains("- Tools: tools"));
        assert!(summary.contains("- Entries: 3"));
        assert!(summary.contains("- Tool calls: 2"));
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
        assert!(body.contains("### User"));
        assert!(body.contains("### Tool Activity"));
        assert!(body.contains("### System"));
        assert!(body.contains("Turn completed"));
        assert!(!body.contains("--- [Tool: grep]"));
        assert!(!body.contains("--- [Tool: read_file]"));
    }

    #[test]
    fn conversation_export_headings_cover_assistant_system_and_error_blocks() {
        let entries = vec![
            ChatEntry::new(ChatRole::Assistant, "Final answer".to_string()),
            ChatEntry::new(
                ChatRole::System,
                "Session memory updated · summary · /tmp/live.md".to_string(),
            ),
            ChatEntry::new(ChatRole::Error, "something odd happened".to_string()),
        ];
        let body = render_conversation_body(&entries);
        assert!(body.contains("### Assistant"));
        assert!(body.contains("### System"));
        assert!(body.contains("### Error"));
    }

    #[test]
    fn print_export_regression_snapshot() {
        let workspace_index = render_workspace_index(
            std::path::Path::new("/tmp/bundle"),
            std::path::Path::new("/tmp"),
            Some(&state()),
            &Vec::<RuntimeTask>::new(),
            std::path::Path::new("/tmp/bundle/conversation.txt"),
            std::path::Path::new("/tmp/bundle/runtime-summary.txt"),
            std::path::Path::new("/tmp/bundle/runtime-timeline.txt"),
            std::path::Path::new("/tmp/bundle/prompt-cache.txt"),
            None,
            "workflow",
            "coordinate",
            "timeline",
        );
        let completion = render_bundle_completion_message(
            std::path::Path::new("/tmp/bundle"),
            "runtime",
            "context",
            "tools",
            std::path::Path::new("/tmp/bundle/conversation.txt"),
            std::path::Path::new("/tmp/bundle/runtime-summary.txt"),
            std::path::Path::new("/tmp/bundle/workspace-index.md"),
            std::path::Path::new("/tmp/bundle/prompt-cache.txt"),
            &["a".to_string(), "b".to_string()],
            None,
        );
        let transcript_summary = render_conversation_summary_block(
            "runtime", "context", "tools", 3, 10, 20, 30, 2, 4, 1,
        );

        println!("# Export Regression Snapshot\n");
        println!("## Workspace Index\n");
        println!("{}", workspace_index);
        println!("\n## Bundle Completion\n");
        println!("{}", completion);
        println!("\n## Transcript Summary\n");
        println!("{}", transcript_summary);
    }

    #[test]
    fn workspace_index_groups_inspect_targets() {
        let rendered = render_workspace_index(
            std::path::Path::new("/tmp/bundle"),
            std::path::Path::new("/tmp"),
            Some(&state()),
            &Vec::<RuntimeTask>::new(),
            std::path::Path::new("/tmp/bundle/conversation.txt"),
            std::path::Path::new("/tmp/bundle/runtime-summary.txt"),
            std::path::Path::new("/tmp/bundle/runtime-timeline.txt"),
            std::path::Path::new("/tmp/bundle/prompt-cache.txt"),
            None,
            "workflow",
            "coordinate",
            "timeline",
        );
        assert!(rendered.contains("- Overview: /inspect artifact summary · bundle"));
        assert!(rendered.contains(
            "- Flow: /inspect artifact latest-workflow · latest-coordinate · latest-orchestration"
        ));
        assert!(rendered.contains("- Runtime: /inspect artifact latest-runtime-timeline · latest-prompt-cache · latest-prompt-cache-state"));
        assert!(rendered.contains("- Cache: /inspect artifact latest-prompt-cache-events · latest-media-compact-events · latest-prompt-cache-break · latest-prompt-cache-diff"));
        assert!(rendered.contains(
            "- MCP: /inspect artifact latest-mcp-resource · latest-mcp-resource-index · history mcp-resources"
        ));
        assert!(rendered.contains("- Refs: /inspect artifact latest-provider-inventory · latest-review · latest-transcript"));
    }

    #[test]
    fn workspace_index_summarizes_latest_mcp_resource_manifest() {
        let dir = std::env::temp_dir().join(format!("yode-export-index-{}", uuid::Uuid::new_v4()));
        let artifacts = dir.join(".yode").join("status").join("mcp-resources");
        let bundle = dir.join("bundle");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&artifacts).unwrap();
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(
            bundle.join("mcp-resources-index.md"),
            "# MCP Resource Artifacts\n",
        )
        .unwrap();
        std::fs::write(
            artifacts.join("session-mcp-resource-demo.md"),
            "# MCP Resource Blob Artifact\n\n- Server: demo\n- URI: mcp://image\n- Blob count: 1\n\n## Blob 1\n\n- Decode warning: invalid base64\n",
        )
        .unwrap();

        let rendered = render_workspace_index(
            &bundle,
            &dir,
            Some(&state()),
            &Vec::<RuntimeTask>::new(),
            std::path::Path::new("/tmp/bundle/conversation.txt"),
            std::path::Path::new("/tmp/bundle/runtime-summary.txt"),
            std::path::Path::new("/tmp/bundle/runtime-timeline.txt"),
            std::path::Path::new("/tmp/bundle/prompt-cache.txt"),
            None,
            "workflow",
            "coordinate",
            "timeline",
        );
        assert!(rendered.contains("- resource: ["));
        assert!(rendered.contains("server=demo"));
        assert!(rendered.contains("uri=mcp://image"));
        assert!(rendered.contains("blobs=1"));
        assert!(rendered.contains("decode_warnings=1"));
        assert!(rendered.contains("resource index:"));
        assert!(rendered.contains("mcp-resources-index.md"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mcp_resource_artifact_candidates_include_manifest_base64_and_decoded_files() {
        let dir = std::env::temp_dir().join(format!("yode-export-mcp-{}", uuid::Uuid::new_v4()));
        let artifacts = dir.join(".yode").join("status").join("mcp-resources");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&artifacts).unwrap();
        std::fs::write(artifacts.join("session-mcp-resource-demo.md"), "manifest").unwrap();
        std::fs::write(artifacts.join("session-mcp-resource-demo.b64"), "ZmFrZQ==").unwrap();
        std::fs::write(artifacts.join("session-mcp-resource-demo.png"), b"fake").unwrap();

        let paths = mcp_resource_artifact_candidates(&dir, 12);
        assert_eq!(paths.len(), 3);
        assert!(paths
            .iter()
            .any(|path| path.extension().is_some_and(|ext| ext == "md")));
        assert!(paths
            .iter()
            .any(|path| path.extension().is_some_and(|ext| ext == "b64")));
        assert!(paths
            .iter()
            .any(|path| path.extension().is_some_and(|ext| ext == "png")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mcp_resource_artifact_path_detection_matches_resource_dir_only() {
        assert!(is_mcp_resource_artifact(std::path::Path::new(
            "/tmp/project/.yode/status/mcp-resources/a.md"
        )));
        assert!(!is_mcp_resource_artifact(std::path::Path::new(
            "/tmp/project/.yode/status/a-mcp-resource.md"
        )));
    }

    #[test]
    fn mcp_resource_export_index_writes_manifest_summary() {
        let dir =
            std::env::temp_dir().join(format!("yode-export-mcp-index-{}", uuid::Uuid::new_v4()));
        let resources = dir.join(".yode").join("status").join("mcp-resources");
        let bundle = dir.join("bundle");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&resources).unwrap();
        std::fs::create_dir_all(&bundle).unwrap();
        let manifest = resources.join("session-mcp-resource-demo.md");
        std::fs::write(
            &manifest,
            "# MCP Resource Blob Artifact\n\n- Server: demo\n- URI: mcp://image\n- Blob count: 1\n- Retention: keep newest 120 artifact files\n\n## Blob 1\n\n- Decode warning: invalid base64\n",
        )
        .unwrap();

        let index =
            write_mcp_resource_export_index(&bundle, &[manifest]).expect("index should be written");
        let content = std::fs::read_to_string(index).unwrap();
        assert!(content.contains("server=demo"));
        assert!(content.contains("uri=mcp://image"));
        assert!(content.contains("retention=keep newest 120 artifact files"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn conversation_export_preserves_plain_url_text() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::Assistant,
                "See https://example.com/docs".to_string(),
            ),
            ChatEntry::new(
                ChatRole::System,
                "Session memory updated · ref · https://docs.rs".to_string(),
            ),
        ];
        let body = render_conversation_body(&entries);
        assert!(body.contains("https://example.com/docs"));
        assert!(body.contains("https://docs.rs"));
    }

    #[test]
    fn export_and_bundle_copy_use_distinct_nouns() {
        let completion = render_bundle_completion_message(
            std::path::Path::new("/tmp/bundle"),
            "runtime",
            "context",
            "tools",
            std::path::Path::new("/tmp/bundle/conversation.txt"),
            std::path::Path::new("/tmp/bundle/runtime-summary.txt"),
            std::path::Path::new("/tmp/bundle/workspace-index.md"),
            std::path::Path::new("/tmp/bundle/prompt-cache.txt"),
            &[],
            None,
        );
        assert!(completion.contains("Diagnostics bundle written:"));
        assert!(!completion.contains("Diagnostics export"));
    }

    #[test]
    fn bundle_copy_keeps_compact_punctuation_spacing() {
        let completion = render_bundle_completion_message(
            std::path::Path::new("/tmp/bundle"),
            "runtime",
            "context",
            "tools",
            std::path::Path::new("/tmp/bundle/conversation.txt"),
            std::path::Path::new("/tmp/bundle/runtime-summary.txt"),
            std::path::Path::new("/tmp/bundle/workspace-index.md"),
            std::path::Path::new("/tmp/bundle/prompt-cache.txt"),
            &[],
            None,
        );
        assert!(!completion.contains(" :"));
    }
}
