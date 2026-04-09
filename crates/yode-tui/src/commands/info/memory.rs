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
                    hint: "[live|session|latest|list [recent|auto|manual|summary|failed]|compare <a> <b>|<index>|<file>]".to_string(),
                    completions: ArgCompletionSource::Static(vec![
                        "live".to_string(),
                        "session".to_string(),
                        "latest".to_string(),
                        "list".to_string(),
                        "list recent".to_string(),
                        "list auto".to_string(),
                        "list manual".to_string(),
                        "list summary".to_string(),
                        "list failed".to_string(),
                        "compare".to_string(),
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

        let args = args.trim();

        if args == "compare" || args.starts_with("compare ") {
            let compare = parse_compare_targets(args)
                .ok_or_else(|| "Usage: /memory compare <a> <b>".to_string())?;
            return render_transcript_compare(&transcripts_dir, compare.0, compare.1);
        }

        match args {
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
                    .ok_or_else(|| {
                        "No transcript backups found. Transcript artifacts are written only after a compaction that actually removes or truncates content.".to_string()
                    })?;
                render_latest_transcript(&latest)
            }
            "list" => Ok(CommandOutput::Message(render_transcript_list(
                &transcripts_dir,
                TranscriptFilter::All,
            ))),
            "list recent" => Ok(CommandOutput::Message(render_transcript_list(
                &transcripts_dir,
                TranscriptFilter::Recent,
            ))),
            "list auto" => Ok(CommandOutput::Message(render_transcript_list(
                &transcripts_dir,
                TranscriptFilter::Auto,
            ))),
            "list manual" => Ok(CommandOutput::Message(render_transcript_list(
                &transcripts_dir,
                TranscriptFilter::Manual,
            ))),
            "list summary" => Ok(CommandOutput::Message(render_transcript_list(
                &transcripts_dir,
                TranscriptFilter::Summary,
            ))),
            "list failed" => Ok(CommandOutput::Message(render_transcript_list(
                &transcripts_dir,
                TranscriptFilter::Failed,
            ))),
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
        "Memory artifacts:\n  Live memory:       {}\n  Compaction memory: {}\n  Transcript dir:    {}\n  Transcript count:  {}\n  Latest transcript: {}{}\n\nUse /memory live, /memory session, /memory latest, /memory list, /memory compare <a> <b>, or /memory <index>.",
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
    let content =
        fs::read_to_string(path).map_err(|_| format!("{} not found: {}", label, path.display()))?;
    let truncated = truncate_for_display(&content);
    Ok(CommandOutput::Message(format!(
        "{}\nPath: {}\n\n{}",
        label,
        path.display(),
        truncated
    )))
}

fn render_latest_transcript(path: &Path) -> CommandResult {
    let content = fs::read_to_string(path)
        .map_err(|_| format!("Latest transcript not found: {}", path.display()))?;
    let meta = read_transcript_metadata(path).unwrap_or_default();
    let preview =
        extract_summary_preview(&content).unwrap_or_else(|| "No summary anchor".to_string());
    let truncated = truncate_for_display(&content);
    Ok(CommandOutput::Message(format!(
        "Latest transcript\nPath: {}\nMode: {}\nTimestamp: {}\nRemoved: {}\nTruncated: {}\nFailed tool results: {}\nSummary preview: {}\n\n{}",
        path.display(),
        meta.mode.unwrap_or_else(|| "unknown".to_string()),
        meta.timestamp.unwrap_or_else(|| "unknown".to_string()),
        meta.removed.unwrap_or(0),
        meta.truncated.unwrap_or(0),
        meta.failed_tool_results.unwrap_or(0),
        preview,
        truncated
    )))
}

fn render_transcript_compare(dir: &Path, left_target: &str, right_target: &str) -> CommandResult {
    let left_path = resolve_compare_target(dir, left_target)
        .ok_or_else(|| format!("Unknown compare target: {}", left_target))?;
    let right_path = resolve_compare_target(dir, right_target)
        .ok_or_else(|| format!("Unknown compare target: {}", right_target))?;
    let left_content = fs::read_to_string(&left_path)
        .map_err(|_| format!("Transcript not found: {}", left_path.display()))?;
    let right_content = fs::read_to_string(&right_path)
        .map_err(|_| format!("Transcript not found: {}", right_path.display()))?;
    Ok(CommandOutput::Message(build_transcript_compare_output(
        &left_path,
        &left_content,
        &right_path,
        &right_content,
    )))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TranscriptFilter {
    All,
    Recent,
    Auto,
    Manual,
    Summary,
    Failed,
}

fn render_transcript_list(dir: &Path, filter: TranscriptFilter) -> String {
    let entries = filtered_transcript_entries(dir, filter);
    if entries.is_empty() {
        return format!(
            "No transcript backups found in {}. Transcript artifacts are written only after a compaction that actually removes or truncates content.",
            dir.display()
        );
    }

    let label = match filter {
        TranscriptFilter::All => "all",
        TranscriptFilter::Recent => "recent",
        TranscriptFilter::Auto => "auto",
        TranscriptFilter::Manual => "manual",
        TranscriptFilter::Summary => "summary",
        TranscriptFilter::Failed => "failed",
    };
    let mut output = format!("Transcript backups in {} ({label}):\n", dir.display());
    for (idx, path) in entries.into_iter().take(10).enumerate() {
        output.push_str(&format!("  {:>2}. ", idx + 1));
        output.push_str(&path.display().to_string());
        if let Some(meta) = read_transcript_metadata(&path) {
            output.push_str(&format!(
                "\n      {} | mode={} | removed={} | truncated={} | failed={}{}",
                meta.timestamp.unwrap_or_else(|| "unknown time".to_string()),
                meta.mode.unwrap_or_else(|| "unknown".to_string()),
                meta.removed.unwrap_or(0),
                meta.truncated.unwrap_or(0),
                meta.failed_tool_results.unwrap_or(0),
                if meta.has_summary {
                    " | summary=yes"
                } else {
                    ""
                }
            ));
        }
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

fn filtered_transcript_entries(dir: &Path, filter: TranscriptFilter) -> Vec<PathBuf> {
    let mut entries = sorted_transcript_entries(dir);
    match filter {
        TranscriptFilter::All => {}
        TranscriptFilter::Recent => {
            entries.truncate(5);
        }
        TranscriptFilter::Auto
        | TranscriptFilter::Manual
        | TranscriptFilter::Summary
        | TranscriptFilter::Failed => {
            entries.retain(|path| {
                let meta = match read_transcript_metadata(path) {
                    Some(meta) => meta,
                    None => return false,
                };
                match filter {
                    TranscriptFilter::Auto => meta.mode.as_deref() == Some("auto"),
                    TranscriptFilter::Manual => meta.mode.as_deref() == Some("manual"),
                    TranscriptFilter::Summary => meta.has_summary,
                    TranscriptFilter::Failed => meta.failed_tool_results.unwrap_or(0) > 0,
                    _ => true,
                }
            });
        }
    }
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

fn resolve_compare_target(dir: &Path, target: &str) -> Option<PathBuf> {
    if target == "latest" {
        latest_transcript(dir)
    } else {
        resolve_transcript_target(dir, target)
    }
}

fn parse_compare_targets(args: &str) -> Option<(&str, &str)> {
    let rest = args.strip_prefix("compare ")?;
    let mut parts = rest.split_whitespace();
    let left = parts.next()?;
    let right = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    Some((left, right))
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

#[derive(Debug, Default)]
struct TranscriptMetadata {
    timestamp: Option<String>,
    mode: Option<String>,
    removed: Option<usize>,
    truncated: Option<usize>,
    failed_tool_results: Option<usize>,
    has_summary: bool,
}

fn read_transcript_metadata(path: &Path) -> Option<TranscriptMetadata> {
    let content = fs::read_to_string(path).ok()?;
    let mut meta = TranscriptMetadata::default();

    for line in content.lines().take(10) {
        if let Some(value) = line.strip_prefix("- Timestamp: ") {
            meta.timestamp = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("- Mode: ") {
            meta.mode = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("- Removed messages: ") {
            meta.removed = value.parse::<usize>().ok();
        } else if let Some(value) = line.strip_prefix("- Tool results truncated: ") {
            meta.truncated = value.parse::<usize>().ok();
        } else if let Some(value) = line.strip_prefix("- Failed tool results: ") {
            meta.failed_tool_results = value.parse::<usize>().ok();
        }
    }

    meta.has_summary = content.contains("## Summary Anchor");
    Some(meta)
}

fn extract_summary_preview(content: &str) -> Option<String> {
    let start = content.find("## Summary Anchor")?;
    let summary_block = &content[start..];
    let fenced_start = summary_block.find("```text")?;
    let after_fence = &summary_block[fenced_start + "```text".len()..];
    let fenced_end = after_fence.find("```")?;
    let summary = after_fence[..fenced_end].trim();
    if summary.is_empty() {
        return None;
    }

    let preview: String = summary.chars().take(180).collect();
    if summary.chars().count() > 180 {
        Some(format!("{}...", preview))
    } else {
        Some(preview)
    }
}

fn build_transcript_compare_output(
    left_path: &Path,
    left_content: &str,
    right_path: &Path,
    right_content: &str,
) -> String {
    let left_meta = read_transcript_metadata(left_path).unwrap_or_default();
    let right_meta = read_transcript_metadata(right_path).unwrap_or_default();
    let left_summary = extract_summary_preview(left_content).unwrap_or_else(|| "none".to_string());
    let right_summary =
        extract_summary_preview(right_content).unwrap_or_else(|| "none".to_string());
    let left_lines = left_content.lines().count();
    let right_lines = right_content.lines().count();
    let left_chars = left_content.chars().count();
    let right_chars = right_content.chars().count();
    let left_messages = count_transcript_messages(left_content);
    let right_messages = count_transcript_messages(right_content);
    let identical = left_content == right_content;

    let mut output = String::new();
    output.push_str("Transcript comparison\n");
    output.push_str(&format!("A: {}\n", left_path.display()));
    output.push_str(&format!("B: {}\n", right_path.display()));
    output.push_str(&format!(
        "Status: {}\n\n",
        if identical { "identical" } else { "different" }
    ));

    output.push_str("Metadata:\n");
    output.push_str(&format_compare_field(
        "Mode",
        left_meta.mode.as_deref().unwrap_or("unknown"),
        right_meta.mode.as_deref().unwrap_or("unknown"),
    ));
    output.push_str(&format_compare_field(
        "Timestamp",
        left_meta.timestamp.as_deref().unwrap_or("unknown"),
        right_meta.timestamp.as_deref().unwrap_or("unknown"),
    ));
    output.push_str(&format_compare_field(
        "Removed",
        &left_meta.removed.unwrap_or(0).to_string(),
        &right_meta.removed.unwrap_or(0).to_string(),
    ));
    output.push_str(&format_compare_field(
        "Truncated",
        &left_meta.truncated.unwrap_or(0).to_string(),
        &right_meta.truncated.unwrap_or(0).to_string(),
    ));
    output.push_str(&format_compare_field(
        "Failed tool results",
        &left_meta.failed_tool_results.unwrap_or(0).to_string(),
        &right_meta.failed_tool_results.unwrap_or(0).to_string(),
    ));
    output.push_str(&format_compare_field(
        "Summary anchor",
        if left_meta.has_summary { "yes" } else { "no" },
        if right_meta.has_summary { "yes" } else { "no" },
    ));
    output.push_str(&format_compare_field(
        "Message sections",
        &left_messages.to_string(),
        &right_messages.to_string(),
    ));
    output.push_str(&format_compare_field(
        "Lines",
        &left_lines.to_string(),
        &right_lines.to_string(),
    ));
    output.push_str(&format_compare_field(
        "Chars",
        &left_chars.to_string(),
        &right_chars.to_string(),
    ));

    output.push_str("\nSummary preview:\n");
    output.push_str(&format!("  A: {}\n", left_summary));
    output.push_str(&format!("  B: {}\n", right_summary));

    if let Some((line_no, left_line, right_line)) = first_difference(left_content, right_content) {
        output.push_str("\nFirst difference:\n");
        output.push_str(&format!("  Line: {}\n", line_no));
        output.push_str(&format!(
            "  A: {}\n",
            summarize_compare_line(left_line.unwrap_or("<no line>"))
        ));
        output.push_str(&format!(
            "  B: {}\n",
            summarize_compare_line(right_line.unwrap_or("<no line>"))
        ));
    }

    output
}

fn format_compare_field(label: &str, left: &str, right: &str) -> String {
    if left == right {
        format!("  {:<18} {}\n", label, left)
    } else {
        format!("  {:<18} {} -> {}\n", label, left, right)
    }
}

fn count_transcript_messages(content: &str) -> usize {
    content
        .lines()
        .filter(|line| line.starts_with("### "))
        .count()
}

fn first_difference<'a>(
    left: &'a str,
    right: &'a str,
) -> Option<(usize, Option<&'a str>, Option<&'a str>)> {
    let left_lines = left.lines().collect::<Vec<_>>();
    let right_lines = right.lines().collect::<Vec<_>>();
    let max_len = left_lines.len().max(right_lines.len());

    for idx in 0..max_len {
        let left_line = left_lines.get(idx).copied();
        let right_line = right_lines.get(idx).copied();
        if left_line != right_line {
            return Some((idx + 1, left_line, right_line));
        }
    }

    None
}

fn summarize_compare_line(line: &str) -> String {
    let squashed = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if squashed.chars().count() <= 180 {
        return squashed;
    }

    let truncated = squashed.chars().take(180).collect::<String>();
    format!("{}...", truncated)
}

#[cfg(test)]
mod tests {
    use super::{
        build_transcript_compare_output, extract_summary_preview, filtered_transcript_entries,
        latest_transcript, parse_compare_targets, read_transcript_metadata, render_transcript_list,
        resolve_transcript_target, truncate_for_display, TranscriptFilter, MAX_DISPLAY_CHARS,
    };

    #[test]
    fn latest_transcript_prefers_newest_filename() {
        let dir =
            std::env::temp_dir().join(format!("yode-memory-command-test-{}", uuid::Uuid::new_v4()));
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

        let listing = render_transcript_list(&dir, TranscriptFilter::All);
        assert!(listing.contains("  1. "));
        assert!(listing.contains("  2. "));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn read_transcript_metadata_parses_header_fields() {
        let dir =
            std::env::temp_dir().join(format!("yode-memory-command-meta-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sample.md");
        std::fs::write(
            &path,
            "# Compaction Transcript\n\n- Session: abc\n- Mode: manual\n- Timestamp: 2026-01-01 10:00:00\n- Removed messages: 7\n- Tool results truncated: 2\n- Failed tool results: 1\n\n## Summary Anchor\n",
        )
        .unwrap();

        let meta = read_transcript_metadata(&path).unwrap();
        assert_eq!(meta.mode.as_deref(), Some("manual"));
        assert_eq!(meta.timestamp.as_deref(), Some("2026-01-01 10:00:00"));
        assert_eq!(meta.removed, Some(7));
        assert_eq!(meta.truncated, Some(2));
        assert_eq!(meta.failed_tool_results, Some(1));
        assert!(meta.has_summary);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn extract_summary_preview_reads_summary_anchor_block() {
        let content = "# Compaction Transcript\n\n## Summary Anchor\n\n```text\nFirst line\nSecond line\n```\n";
        let preview = extract_summary_preview(content).unwrap();
        assert!(preview.contains("First line"));
        assert!(preview.contains("Second line"));
    }

    #[test]
    fn filtered_transcript_entries_supports_mode_filter() {
        let dir = std::env::temp_dir().join(format!(
            "yode-memory-command-filter-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("auto.md"),
            "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-01 10:00:00\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("manual.md"),
            "# Compaction Transcript\n\n- Mode: manual\n- Timestamp: 2026-01-01 11:00:00\n",
        )
        .unwrap();

        let auto = filtered_transcript_entries(&dir, TranscriptFilter::Auto);
        assert_eq!(auto.len(), 1);
        assert!(auto[0].ends_with("auto.md"));

        let manual_listing = render_transcript_list(&dir, TranscriptFilter::Manual);
        assert!(manual_listing.contains("manual"));
        assert!(!manual_listing.contains("auto.md"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn filtered_transcript_entries_supports_summary_filter() {
        let dir = std::env::temp_dir().join(format!(
            "yode-memory-command-summary-filter-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("with-summary.md"),
            "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-01 10:00:00\n\n## Summary Anchor\n\n```text\nsummary\n```\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("without-summary.md"),
            "# Compaction Transcript\n\n- Mode: manual\n- Timestamp: 2026-01-01 11:00:00\n",
        )
        .unwrap();

        let summary = filtered_transcript_entries(&dir, TranscriptFilter::Summary);
        assert_eq!(summary.len(), 1);
        assert!(summary[0].ends_with("with-summary.md"));

        let summary_listing = render_transcript_list(&dir, TranscriptFilter::Summary);
        assert!(summary_listing.contains("summary=yes"));
        assert!(!summary_listing.contains("without-summary.md"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn filtered_transcript_entries_supports_failed_filter() {
        let dir = std::env::temp_dir().join(format!(
            "yode-memory-command-failed-filter-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("failed.md"),
            "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-01 10:00:00\n- Failed tool results: 2\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("clean.md"),
            "# Compaction Transcript\n\n- Mode: manual\n- Timestamp: 2026-01-01 11:00:00\n- Failed tool results: 0\n",
        )
        .unwrap();

        let failed = filtered_transcript_entries(&dir, TranscriptFilter::Failed);
        assert_eq!(failed.len(), 1);
        assert!(failed[0].ends_with("failed.md"));

        let failed_listing = render_transcript_list(&dir, TranscriptFilter::Failed);
        assert!(failed_listing.contains("failed=2"));
        assert!(!failed_listing.contains("clean.md"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parse_compare_targets_accepts_two_values() {
        assert_eq!(parse_compare_targets("compare 1 2"), Some(("1", "2")));
        assert_eq!(
            parse_compare_targets("compare latest sample.md"),
            Some(("latest", "sample.md"))
        );
        assert_eq!(parse_compare_targets("compare 1"), None);
        assert_eq!(parse_compare_targets("list compare 1 2"), None);
    }

    #[test]
    fn build_transcript_compare_output_highlights_differences() {
        let dir = std::env::temp_dir().join(format!(
            "yode-memory-command-compare-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let left_path = dir.join("left.md");
        let right_path = dir.join("right.md");
        let left = "# Compaction Transcript\n\n- Session: abc\n- Mode: auto\n- Timestamp: 2026-01-01 10:00:00\n- Removed messages: 7\n- Tool results truncated: 1\n- Failed tool results: 1\n\n## Summary Anchor\n\n```text\nLeft summary\n```\n\n## Messages\n\n### User\n\n```text\nhello\n```\n";
        let right = "# Compaction Transcript\n\n- Session: abc\n- Mode: manual\n- Timestamp: 2026-01-01 11:00:00\n- Removed messages: 3\n- Tool results truncated: 0\n- Failed tool results: 0\n\n## Summary Anchor\n\n```text\nRight summary\n```\n\n## Messages\n\n### User\n\n```text\nhello\n```\n";
        std::fs::write(&left_path, left).unwrap();
        std::fs::write(&right_path, right).unwrap();

        let output = build_transcript_compare_output(&left_path, left, &right_path, right);
        assert!(output.contains("Status: different"));
        assert!(output.contains("Mode               auto -> manual"));
        assert!(output.contains("Failed tool results 1 -> 0"));
        assert!(output.contains("A: Left summary"));
        assert!(output.contains("B: Right summary"));
        assert!(output.contains("First difference:"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
