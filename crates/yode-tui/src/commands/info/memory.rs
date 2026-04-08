use std::fs;
use std::path::{Path, PathBuf};

use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

const MAX_DISPLAY_CHARS: usize = 12_000;

pub struct MemoryCommand {
    meta: CommandMeta,
}

impl MemoryCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "memory",
                description: "Inspect live memory, compacted memory, and transcripts",
                aliases: &["mem"],
                args: vec![ArgDef {
                    name: "target".to_string(),
                    required: false,
                    hint: "[live|session|latest|list|<index>|<file>]".to_string(),
                    completions: ArgCompletionSource::Static(vec![
                        "live".to_string(),
                        "session".to_string(),
                        "latest".to_string(),
                        "list".to_string(),
                    ]),
                }],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for MemoryCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let project_root = PathBuf::from(&ctx.session.working_dir);
        let live_path = yode_core::session_memory::live_session_memory_path(&project_root);
        let session_path = yode_core::session_memory::session_memory_path(&project_root);
        let transcripts_dir = project_root.join(".yode").join("transcripts");
        let runtime = ctx
            .engine
            .try_lock()
            .ok()
            .map(|engine| engine.runtime_state());

        match args.trim() {
            "" => Ok(CommandOutput::Message(render_memory_status(
                &live_path,
                &session_path,
                &transcripts_dir,
                runtime.as_ref(),
            ))),
            "live" => render_file("Live session memory", &live_path),
            "session" => render_file("Compaction memory", &session_path),
            "latest" => {
                let latest = latest_transcript(&transcripts_dir)
                    .ok_or_else(|| "No transcript backups found.".to_string())?;
                render_file("Latest transcript", &latest)
            }
            "list" => Ok(CommandOutput::Message(render_transcript_list(&transcripts_dir))),
            target => {
                let transcript = resolve_transcript_target(&transcripts_dir, target)
                    .ok_or_else(|| format!("Unknown memory target: {}", target))?;
                render_file("Transcript", &transcript)
            }
        }
    }
}

fn render_memory_status(
    live_path: &Path,
    session_path: &Path,
    transcripts_dir: &Path,
    runtime: Option<&yode_core::engine::EngineRuntimeState>,
) -> String {
    let latest = latest_transcript(transcripts_dir);
    let transcript_count = transcript_entries(transcripts_dir).len();
    let runtime_lines = runtime
        .map(|state| {
            format!(
                "\n  Last compact mode: {}\n  Last compact at:   {}\n  Last compact summary: {}\n  Last compact mem:  {}\n  Last transcript:   {}\n  Last memory update: {}",
                state
                    .last_compaction_mode
                    .as_deref()
                    .unwrap_or("none"),
                state
                    .last_compaction_at
                    .as_deref()
                    .unwrap_or("none"),
                state
                    .last_compaction_summary_excerpt
                    .as_deref()
                    .unwrap_or("none"),
                state
                    .last_compaction_session_memory_path
                    .as_deref()
                    .unwrap_or("none"),
                state
                    .last_compaction_transcript_path
                    .as_deref()
                    .unwrap_or("none"),
                state
                    .last_session_memory_update_path
                    .as_ref()
                    .map(|path| {
                        format!(
                            "{} ({}, {})",
                            path,
                            state
                                .last_session_memory_update_at
                                .as_deref()
                                .unwrap_or("unknown time"),
                            if state.last_session_memory_generated_summary {
                                "summary"
                            } else {
                                "snapshot"
                            }
                        )
                    })
                    .unwrap_or_else(|| "none".to_string()),
            )
        })
        .unwrap_or_default();
    format!(
        "Memory artifacts:\n  Live memory:       {}\n  Compaction memory: {}\n  Transcript dir:    {}\n  Transcript count:  {}\n  Latest transcript: {}{}\n\nUse /memory live, /memory session, /memory latest, /memory list, or /memory <index>.",
        describe_path(live_path),
        describe_path(session_path),
        transcripts_dir.display(),
        transcript_count,
        latest
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "none".to_string()),
        runtime_lines,
    )
}

fn render_file(label: &str, path: &Path) -> CommandResult {
    let content = fs::read_to_string(path)
        .map_err(|_| format!("{} not found: {}", label, path.display()))?;
    let truncated = truncate_for_display(&content);
    Ok(CommandOutput::Message(format!(
        "{}\nPath: {}\n\n{}",
        label,
        path.display(),
        truncated
    )))
}

fn render_transcript_list(dir: &Path) -> String {
    let entries = sorted_transcript_entries(dir);
    if entries.is_empty() {
        return format!("No transcript backups found in {}.", dir.display());
    }

    let mut output = format!("Transcript backups in {}:\n", dir.display());
    for (idx, path) in entries.into_iter().take(10).enumerate() {
        output.push_str(&format!("  {:>2}. ", idx + 1));
        output.push_str(&path.display().to_string());
        output.push('\n');
    }
    output
}

fn latest_transcript(dir: &Path) -> Option<PathBuf> {
    sorted_transcript_entries(dir).into_iter().next()
}

fn transcript_entries(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .collect()
}

fn sorted_transcript_entries(dir: &Path) -> Vec<PathBuf> {
    let mut entries = transcript_entries(dir);
    entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    entries
}

fn resolve_transcript_target(dir: &Path, target: &str) -> Option<PathBuf> {
    let entries = sorted_transcript_entries(dir);
    if let Ok(index) = target.parse::<usize>() {
        if index == 0 {
            return None;
        }
        return entries.get(index - 1).cloned();
    }

    entries.into_iter().find(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name == target)
            .unwrap_or(false)
            || path.display().to_string() == target
    })
}

fn describe_path(path: &Path) -> String {
    match fs::metadata(path) {
        Ok(meta) => format!("{} ({} bytes)", path.display(), meta.len()),
        Err(_) => format!("{} (missing)", path.display()),
    }
}

fn truncate_for_display(content: &str) -> String {
    if content.chars().count() <= MAX_DISPLAY_CHARS {
        return content.to_string();
    }

    let truncated = content.chars().take(MAX_DISPLAY_CHARS).collect::<String>();
    format!(
        "{}\n\n[Truncated for display at {} chars]",
        truncated, MAX_DISPLAY_CHARS
    )
}

#[cfg(test)]
mod tests {
    use super::{
        latest_transcript, render_transcript_list, resolve_transcript_target,
        truncate_for_display, MAX_DISPLAY_CHARS,
    };

    #[test]
    fn latest_transcript_prefers_newest_filename() {
        let dir = std::env::temp_dir().join(format!(
            "yode-memory-command-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("aaa-compact-20240101.md"), "old").unwrap();
        std::fs::write(dir.join("bbb-compact-20250101.md"), "new").unwrap();

        let latest = latest_transcript(&dir).unwrap();
        assert!(latest.ends_with("bbb-compact-20250101.md"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn truncate_for_display_appends_notice() {
        let text = "x".repeat(MAX_DISPLAY_CHARS + 100);
        let truncated = truncate_for_display(&text);
        assert!(truncated.contains("Truncated for display"));
        assert!(truncated.len() < text.len());
    }

    #[test]
    fn resolve_transcript_target_supports_index_and_filename() {
        let dir = std::env::temp_dir().join(format!(
            "yode-memory-command-resolve-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("aaa-compact-20240101.md"), "old").unwrap();
        std::fs::write(dir.join("bbb-compact-20250101.md"), "new").unwrap();

        let first = resolve_transcript_target(&dir, "1").unwrap();
        assert!(first.ends_with("bbb-compact-20250101.md"));

        let by_name = resolve_transcript_target(&dir, "aaa-compact-20240101.md").unwrap();
        assert!(by_name.ends_with("aaa-compact-20240101.md"));

        let listing = render_transcript_list(&dir);
        assert!(listing.contains("  1. "));
        assert!(listing.contains("  2. "));

        std::fs::remove_dir_all(&dir).ok();
    }
}
