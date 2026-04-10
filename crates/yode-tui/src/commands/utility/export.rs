use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use crate::app::ChatRole;
use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
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
                    sanitize_filename(text)
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

fn export_diagnostics_bundle(
    custom_name: Option<&str>,
    ctx: &mut CommandContext,
) -> CommandResult {
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
    let runtime = ctx
        .engine
        .try_lock()
        .ok()
        .map(|engine| engine.runtime_state());
    let runtime_summary = if let Some(state) = runtime {
        format!(
            "Runtime summary\n  Query source: {}\n  Tool calls: {}\n  Tool progress: {}\n  Parallel batches: {}\n  Last tool artifact: {}\n  Last transcript: {}\n  Last compact summary: {}\n  Prompt cache turns: {}\n  System prompt est tokens: {}\n",
            state.query_source,
            state.session_tool_calls_total,
            state.tool_progress_event_count,
            state.parallel_tool_batch_count,
            state.last_tool_turn_artifact_path.unwrap_or_else(|| "none".to_string()),
            state.last_compaction_transcript_path.unwrap_or_else(|| "none".to_string()),
            state.last_compaction_summary_excerpt.unwrap_or_else(|| "none".to_string()),
            state.prompt_cache.reported_turns,
            state.system_prompt_estimated_tokens,
        )
    } else {
        "Runtime summary unavailable: engine busy.".to_string()
    };
    std::fs::write(&diagnostics_path, runtime_summary)
        .map_err(|err| format!("Failed to write {}: {}", diagnostics_path.display(), err))?;

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

    Ok(CommandOutput::Message(format!(
        "Diagnostics bundle exported to: {}\n  Conversation: {}\n  Runtime: {}\n  Copied artifacts: {}",
        bundle_dir.display(),
        conversation_path.display(),
        diagnostics_path.display(),
        if copied.is_empty() {
            "none".to_string()
        } else {
            copied.join(", ")
        }
    )))
}

fn latest_artifact_candidates(ctx: &mut CommandContext) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(engine) = ctx.engine.try_lock() {
        let runtime = engine.runtime_state();
        for maybe_path in [
            runtime.last_tool_turn_artifact_path,
            runtime.last_compaction_transcript_path,
            runtime.last_compaction_session_memory_path,
            runtime.last_recovery_artifact_path,
            runtime.last_permission_artifact_path,
        ] {
            if let Some(path) = maybe_path {
                paths.push(PathBuf::from(path));
            }
        }
    }

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
    paths
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
