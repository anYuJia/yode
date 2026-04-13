use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use crate::app::ChatRole;
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

    for entry in ctx.chat_entries.iter() {
        let role_label = match &entry.role {
            ChatRole::User => "User",
            ChatRole::Assistant => "Assistant",
            ChatRole::ToolCall { name, .. } => &format!("[Tool: {}]", name),
            ChatRole::ToolResult { name, is_error, .. } => {
                if *is_error {
                    &format!("[Tool Error: {}]", name)
                } else {
                    &format!("[Tool Result: {}]", name)
                }
            }
            ChatRole::Error => "[Error]",
            ChatRole::System => "[System]",
            ChatRole::SubAgentCall { description } => &format!("[SubAgent: {}]", description),
            ChatRole::SubAgentToolCall { name } => &format!("[SubAgent Tool: {}]", name),
            ChatRole::SubAgentResult => "[SubAgent Result]",
            ChatRole::AskUser { id } => &format!("[AskUser: {}]", id),
        };

        output.push_str(&format!("--- {}\n", role_label));
        output.push_str(&entry.content);
        output.push_str("\n\n");
    }

    // Add stats summary
    output.push_str(&"=".repeat(60));
    output.push_str("\n\n");
    output.push_str("Statistics:\n");
    output.push_str(&format!("  Input tokens:  {}\n", ctx.session.input_tokens));
    output.push_str(&format!("  Output tokens: {}\n", ctx.session.output_tokens));
    output.push_str(&format!("  Total tokens:  {}\n", ctx.session.total_tokens));
    output.push_str(&format!(
        "  Tool calls:    {}\n",
        ctx.session.tool_call_count
    ));

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
    let runtime_summary = if let Some(ref state) = runtime {
        format!(
            "Runtime summary\n  Query source: {}\n  Tool calls: {}\n  Tool progress: {}\n  Parallel batches: {}\n  Last tool artifact: {}\n  Last transcript: {}\n  Last compact summary: {}\n  Prompt cache turns: {}\n  System prompt est tokens: {}\n",
            state.query_source,
            state.session_tool_calls_total,
            state.tool_progress_event_count,
            state.parallel_tool_batch_count,
            state
                .last_tool_turn_artifact_path
                .as_deref()
                .unwrap_or("none"),
            state
                .last_compaction_transcript_path
                .as_deref()
                .unwrap_or("none"),
            state
                .last_compaction_summary_excerpt
                .as_deref()
                .unwrap_or("none"),
            state.prompt_cache.reported_turns,
            state.system_prompt_estimated_tokens,
        )
    } else {
        "Runtime summary unavailable: engine busy.".to_string()
    };
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
    let workflow_artifact = latest_workflow_execution_artifact(&PathBuf::from(&ctx.session.working_dir))
        .map(|path| format!("[{}] {}", artifact_freshness_badge(&path), path.display()))
        .unwrap_or_else(|| "none".to_string());
    let coordinator_artifact = latest_coordinator_artifact(&PathBuf::from(&ctx.session.working_dir))
        .map(|path| format!("[{}] {}", artifact_freshness_badge(&path), path.display()))
        .unwrap_or_else(|| "none".to_string());
    let orchestration_artifact =
        latest_runtime_orchestration_artifact(&PathBuf::from(&ctx.session.working_dir))
            .map(|path| format!("[{}] {}", artifact_freshness_badge(&path), path.display()))
            .unwrap_or_else(|| "none".to_string());
    let workspace_body = format!(
        "# Workspace Index\n\n- Bundle: {}\n- Conversation: {}\n- Runtime summary: {}\n- Runtime timeline: {}\n- Doctor refs: {}\n\nJump targets:\n- /tasks latest\n- /memory latest\n- /reviews latest\n- /status\n- /diagnostics\n- /doctor bundle\n\nOrchestration artifacts:\n- workflow: {}\n- coordinator: {}\n- timeline: {}\n\nInspect aliases:\n- /inspect artifact summary\n- /inspect artifact latest-workflow\n- /inspect artifact latest-coordinate\n- /inspect artifact latest-orchestration\n- /inspect artifact latest-runtime-timeline\n- /inspect artifact latest-provider-inventory\n- /inspect artifact latest-review\n- /inspect artifact latest-transcript\n- /inspect artifact bundle\n",
        bundle_dir.display(),
        conversation_path.display(),
        diagnostics_path.display(),
        timeline_path.display(),
        if doctor_refs.is_empty() {
            "none".to_string()
        } else {
            doctor_ref_path.display().to_string()
        },
        workflow_artifact,
        coordinator_artifact,
        orchestration_artifact,
    );
    let _ = std::fs::write(&workspace_index, workspace_body);

    Ok(CommandOutput::Message(format!(
        "Diagnostics bundle exported to: {}\n  Conversation: {}\n  Runtime: {}\n  Copied artifacts: {}\n  Doctor refs: {}\n  Workspace index: {}\n  Inspect: /inspect artifact bundle",
        bundle_dir.display(),
        conversation_path.display(),
        diagnostics_path.display(),
        if copied.is_empty() {
            "none".to_string()
        } else {
            copied.join(", ")
        },
        if doctor_refs.is_empty() {
            "none".to_string()
        } else {
            doctor_ref_path.display().to_string()
        },
        workspace_index.display(),
    )))
}

fn latest_artifact_candidates(ctx: &mut CommandContext) -> Vec<PathBuf> {
    let runtime = ctx
        .engine
        .try_lock()
        .ok()
        .map(|engine| engine.runtime_state());
    let project_root = PathBuf::from(&ctx.session.working_dir);
    let mut paths = latest_artifact_candidates_from_links(&latest_runtime_artifact_links(runtime));
    if let Some(runtime_task_artifact) = write_runtime_task_inventory_artifact(
        &project_root,
        &ctx.session.session_id,
        ctx.engine
            .try_lock()
            .ok()
            .map(|engine| engine.runtime_tasks_snapshot())
            .unwrap_or_default(),
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
