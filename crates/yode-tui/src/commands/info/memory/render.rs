use std::fs;
use std::path::Path;

use crate::commands::{CommandOutput, CommandResult};

use super::compare::{build_transcript_compare_output, CompareArgs};
use super::document::{format_section_items_preview, memory_entry_age, parse_memory_document};
use super::transcripts::{
    describe_path, extract_summary_preview, filtered_transcript_entries, fold_transcript_preview,
    latest_transcript, read_transcript_metadata, resolve_compare_target, sorted_transcript_entries,
    transcript_entries, transcript_picker_summary_preview, truncate_for_display,
    ResumeTranscriptCacheWarmupStats, TranscriptListFilter,
};
use super::MAX_DISPLAY_CHARS;

pub(super) fn render_memory_status(
    live_path: &Path,
    session_path: &Path,
    transcripts_dir: &Path,
    runtime: Option<&yode_core::engine::EngineRuntimeState>,
    resume_warmup: Option<&ResumeTranscriptCacheWarmupStats>,
) -> String {
    let latest = latest_transcript(transcripts_dir);
    let latest_failed = filtered_transcript_entries(
        transcripts_dir,
        &TranscriptListFilter {
            require_failed: true,
            recent_limit: Some(1),
            ..TranscriptListFilter::default()
        },
    )
    .into_iter()
    .next();
    let transcript_count = transcript_entries(transcripts_dir).len();
    let runtime_lines = runtime
        .map(|state| {
            format!(
                "\n  Last compact mode: {}\n  Last compact at:   {}\n  Last compact summary: {}\n  Last compact mem:  {}\n  Last transcript:   {}\n  Last memory update: {}",
                state.last_compaction_mode.as_deref().unwrap_or("none"),
                state.last_compaction_at.as_deref().unwrap_or("none"),
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
    let resume_warmup_line = resume_warmup
        .map(|stats| {
            format!(
                "\n  Resume warmup:    {} transcripts / {} metadata / latest={} / {} ms",
                stats.transcript_count,
                stats.metadata_entries_warmed,
                if stats.latest_lookup_cached {
                    "yes"
                } else {
                    "no"
                },
                stats.duration_ms
            )
        })
        .unwrap_or_default();
    format!(
        "Memory artifacts:\n  Live memory:       {}\n  Compaction memory: {}\n  Transcript dir:    {}\n  Transcript count:  {}\n  Latest transcript: {}\n  Latest failed:     {}{}{}\n\nQuick jumps:\n  /memory latest\n  /memory list failed\n  /memory pick\n  /memory compare <a> <b>\n  /memory <index>",
        describe_path(live_path),
        describe_path(session_path),
        transcripts_dir.display(),
        transcript_count,
        latest
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "none".to_string()),
        latest_failed
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "none".to_string()),
        resume_warmup_line,
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

pub(super) fn render_transcript_file(path: &Path) -> CommandResult {
    let content = fs::read_to_string(path)
        .map_err(|_| format!("Transcript not found: {}", path.display()))?;
    Ok(CommandOutput::Message(format!(
        "Transcript\nPath: {}\n\n{}",
        path.display(),
        fold_transcript_preview(&content)
    )))
}

pub(super) fn render_memory_file(label: &str, path: &Path) -> CommandResult {
    let content =
        fs::read_to_string(path).map_err(|_| format!("{} not found: {}", label, path.display()))?;
    if let Some(parsed) = parse_memory_document(&content) {
        let mut output = String::with_capacity(content.len().min(MAX_DISPLAY_CHARS) + 512);
        output.push_str(&format!("{}\nPath: {}\n", label, path.display()));
        output.push_str("Schema: structured-v1\n");
        output.push_str(&format!("Entries: {}\n", parsed.entries.len()));
        if let Some(entry) = parsed.entries.first() {
            output.push_str(&format!(
                "Latest entry: {}{}\n",
                entry.timestamp.as_deref().unwrap_or("unknown"),
                entry
                    .session_id
                    .as_deref()
                    .map(|id| format!(" ({})", id))
                    .unwrap_or_default()
            ));
            output.push_str(&format!(
                "Age: {}\n",
                memory_entry_age(entry.timestamp.as_deref())
            ));
            output.push_str("\nStructured view:\n");
            for section in &entry.sections {
                output.push_str(&format!(
                        "  {} ({}): {}\n",
                        section.title,
                        section.items.len(),
                        format_section_items_preview(&section.items, 180)
                ));
            }
        }
        output.push_str("\nRaw markdown:\n\n");
        output.push_str(&truncate_for_display(&content));
        if content.lines().count() > 120 || content.chars().count() > MAX_DISPLAY_CHARS {
            output.push_str(
                "\n\n[Long artifact output. Scroll in the terminal for more, or open the file path directly.]",
            );
        }
        return Ok(CommandOutput::Message(output));
    }

    render_file(label, path)
}

pub(super) fn render_latest_transcript(path: &Path) -> CommandResult {
    let content = fs::read_to_string(path)
        .map_err(|_| format!("Latest transcript not found: {}", path.display()))?;
    let meta = read_transcript_metadata(path).unwrap_or_default();
    let preview =
        extract_summary_preview(&content).unwrap_or_else(|| "No summary anchor".to_string());
    let truncated = fold_transcript_preview(&content);
    Ok(CommandOutput::Message(format!(
        "Latest transcript\nPath: {}\nMode: {}\nTimestamp: {}\nRemoved: {}\nTruncated: {}\nFailed tool results: {}\nSession memory path: {}\nFiles read: {}\nFiles modified: {}\nSummary preview: {}\n\n{}",
        path.display(),
        meta.mode.unwrap_or_else(|| "unknown".to_string()),
        meta.timestamp.unwrap_or_else(|| "unknown".to_string()),
        meta.removed.unwrap_or(0),
        meta.truncated.unwrap_or(0),
        meta.failed_tool_results.unwrap_or(0),
        meta.session_memory_path.unwrap_or_else(|| "none".to_string()),
        meta.files_read_summary.unwrap_or_else(|| "none".to_string()),
        meta.files_modified_summary.unwrap_or_else(|| "none".to_string()),
        preview,
        truncated
    )))
}

pub(super) fn render_transcript_picker(dir: &Path) -> String {
    let entries = sorted_transcript_entries(dir);
    if entries.is_empty() {
        return "Transcript picker: no transcript artifacts found yet.".to_string();
    }

    let mut output = String::from("Transcript picker:\n");
    for (idx, path) in entries.into_iter().take(12).enumerate() {
        let meta = read_transcript_metadata(&path).unwrap_or_default();
        output.push_str(&format!(
            "  {:>2}. {} | mode={} | failed={} | summary={} | {}\n",
            idx + 1,
            meta.timestamp.unwrap_or_else(|| "unknown time".to_string()),
            meta.mode.unwrap_or_else(|| "unknown".to_string()),
            meta.failed_tool_results.unwrap_or(0),
            if meta.has_summary { "yes" } else { "no" },
            path.display()
        ));
        if let Some(preview) = transcript_picker_summary_preview(&path) {
            output.push_str(&format!("      preview: {}\n", preview));
        }
    }
    output.push_str("\nUse /memory <index> to open one, or /memory compare <a> <b> to diff two.");
    output
}

pub(super) fn render_transcript_compare(dir: &Path, compare: &CompareArgs) -> CommandResult {
    let left_path = resolve_compare_target(dir, &compare.left_target)
        .ok_or_else(|| format!("Unknown compare target: {}", compare.left_target))?;
    let right_path = resolve_compare_target(dir, &compare.right_target)
        .ok_or_else(|| format!("Unknown compare target: {}", compare.right_target))?;
    let left_content = fs::read_to_string(&left_path)
        .map_err(|_| format!("Transcript not found: {}", left_path.display()))?;
    let right_content = fs::read_to_string(&right_path)
        .map_err(|_| format!("Transcript not found: {}", right_path.display()))?;
    Ok(CommandOutput::Message(build_transcript_compare_output(
        &left_path,
        &left_content,
        &right_path,
        &right_content,
        &compare.options,
    )))
}

pub(super) fn render_transcript_list(dir: &Path, filter: &TranscriptListFilter) -> String {
    let entries = filtered_transcript_entries(dir, filter);
    let label = filter.label();
    if entries.is_empty() {
        return format!(
            "No transcript backups matched '{}' in {}. Transcript artifacts are written only after a compaction that actually removes or truncates content.",
            label,
            dir.display(),
        );
    }

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
