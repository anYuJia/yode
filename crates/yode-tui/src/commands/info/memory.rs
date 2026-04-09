use chrono::{Duration, Local, NaiveDate, NaiveDateTime};
use similar::{ChangeTag, DiffOp, TextDiff};
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
                    hint: "[live|session|latest|list [filters...]|compare <a> <b>|<index>|<file>]"
                        .to_string(),
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
                        "list summary failed".to_string(),
                        "list today".to_string(),
                        "list yesterday".to_string(),
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

        if args == "list" || args.starts_with("list ") {
            let filter = parse_list_filter(args)?;
            return Ok(CommandOutput::Message(render_transcript_list(
                &transcripts_dir,
                &filter,
            )));
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct TranscriptListFilter {
    recent_limit: Option<usize>,
    mode: Option<TranscriptMode>,
    require_summary: bool,
    require_failed: bool,
    date_range: Option<DateRangeFilter>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TranscriptMode {
    Auto,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DateRangeFilter {
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
}

impl DateRangeFilter {
    fn single(date: NaiveDate) -> Self {
        Self {
            start: Some(date),
            end: Some(date),
        }
    }

    fn label(&self) -> String {
        fn fmt(date: NaiveDate) -> String {
            date.format("%Y-%m-%d").to_string()
        }

        match (self.start, self.end) {
            (Some(start), Some(end)) if start == end => fmt(start),
            (Some(start), Some(end)) => format!("{}..{}", fmt(start), fmt(end)),
            (Some(start), None) => format!("{}..", fmt(start)),
            (None, Some(end)) => format!("..{}", fmt(end)),
            (None, None) => "range".to_string(),
        }
    }

    fn contains(&self, date: NaiveDate) -> bool {
        if let Some(start) = self.start {
            if date < start {
                return false;
            }
        }
        if let Some(end) = self.end {
            if date > end {
                return false;
            }
        }
        true
    }
}

impl TranscriptListFilter {
    fn label(&self) -> String {
        let mut parts = Vec::new();

        if let Some(mode) = self.mode {
            parts.push(match mode {
                TranscriptMode::Auto => "auto".to_string(),
                TranscriptMode::Manual => "manual".to_string(),
            });
        }
        if self.require_summary {
            parts.push("summary".to_string());
        }
        if self.require_failed {
            parts.push("failed".to_string());
        }
        if let Some(range) = &self.date_range {
            parts.push(range.label());
        }
        if self.recent_limit.is_some() {
            parts.push("recent".to_string());
        }

        if parts.is_empty() {
            "all".to_string()
        } else {
            parts.join(" ")
        }
    }
}

fn render_transcript_list(dir: &Path, filter: &TranscriptListFilter) -> String {
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

fn filtered_transcript_entries(dir: &Path, filter: &TranscriptListFilter) -> Vec<PathBuf> {
    let mut entries = sorted_transcript_entries(dir);
    let needs_metadata = filter.mode.is_some()
        || filter.require_summary
        || filter.require_failed
        || filter.date_range.is_some();

    if needs_metadata {
        entries.retain(|path| {
            let meta = match read_transcript_metadata(path) {
                Some(meta) => meta,
                None => return false,
            };

            if let Some(mode) = filter.mode {
                let expected = match mode {
                    TranscriptMode::Auto => "auto",
                    TranscriptMode::Manual => "manual",
                };
                if meta.mode.as_deref() != Some(expected) {
                    return false;
                }
            }

            if filter.require_summary && !meta.has_summary {
                return false;
            }

            if filter.require_failed && meta.failed_tool_results.unwrap_or(0) == 0 {
                return false;
            }

            if let Some(range) = &filter.date_range {
                let in_range = meta
                    .timestamp
                    .as_deref()
                    .and_then(parse_transcript_date)
                    .map(|date| range.contains(date))
                    .unwrap_or(false);
                if !in_range {
                    return false;
                }
            }

            true
        });
    }

    if let Some(limit) = filter.recent_limit {
        entries.truncate(limit);
    }

    entries
}

fn parse_list_filter(args: &str) -> Result<TranscriptListFilter, String> {
    if args == "list" {
        return Ok(TranscriptListFilter::default());
    }

    let spec = args
        .strip_prefix("list ")
        .ok_or_else(memory_list_usage)?
        .trim();

    if spec.is_empty() {
        return Ok(TranscriptListFilter::default());
    }

    let mut filter = TranscriptListFilter::default();
    for token in spec.split_whitespace() {
        match token {
            "all" => {}
            "recent" => {
                filter.recent_limit = Some(5);
            }
            "auto" => match filter.mode {
                None => filter.mode = Some(TranscriptMode::Auto),
                Some(TranscriptMode::Auto) => {}
                Some(TranscriptMode::Manual) => return Err(memory_list_usage()),
            },
            "manual" => match filter.mode {
                None => filter.mode = Some(TranscriptMode::Manual),
                Some(TranscriptMode::Manual) => {}
                Some(TranscriptMode::Auto) => return Err(memory_list_usage()),
            },
            "summary" => {
                filter.require_summary = true;
            }
            "failed" => {
                filter.require_failed = true;
            }
            _ => {
                let range = parse_date_range_filter(token).ok_or_else(memory_list_usage)?;
                if filter.date_range.is_some() {
                    return Err(memory_list_usage());
                }
                filter.date_range = Some(range);
            }
        }
    }

    Ok(filter)
}

fn memory_list_usage() -> String {
    "Usage: /memory list [recent] [auto|manual] [summary] [failed] [today|yesterday|YYYY-MM-DD|YYYY-MM-DD..YYYY-MM-DD|..YYYY-MM-DD|YYYY-MM-DD..]".to_string()
}

fn parse_date_range_filter(spec: &str) -> Option<DateRangeFilter> {
    if spec == "today" {
        return Some(DateRangeFilter::single(Local::now().date_naive()));
    }
    if spec == "yesterday" {
        return Some(DateRangeFilter::single(
            Local::now().date_naive() - Duration::days(1),
        ));
    }

    if let Some((start, end)) = spec.split_once("..") {
        let start = if start.is_empty() {
            None
        } else {
            Some(parse_iso_date(start)?)
        };
        let end = if end.is_empty() {
            None
        } else {
            Some(parse_iso_date(end)?)
        };
        if start.is_none() && end.is_none() {
            return None;
        }
        if let (Some(start), Some(end)) = (start, end) {
            if start > end {
                return None;
            }
        }
        return Some(DateRangeFilter { start, end });
    }

    parse_iso_date(spec).map(DateRangeFilter::single)
}

fn parse_iso_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
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

fn parse_transcript_date(timestamp: &str) -> Option<NaiveDate> {
    NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|ts| ts.date())
        .or_else(|| parse_iso_date(timestamp))
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

    if let Some(diff_preview) = build_diff_preview(left_content, right_content) {
        output.push_str("\nContent diff:\n");
        output.push_str(&diff_preview);
    }

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

fn build_diff_preview(left: &str, right: &str) -> Option<String> {
    let diff = TextDiff::from_lines(left, right);
    let groups = diff.grouped_ops(2);

    let mut added = 0usize;
    let mut removed = 0usize;
    for op in diff.ops() {
        for change in diff.iter_changes(op) {
            match change.tag() {
                ChangeTag::Insert => added += 1,
                ChangeTag::Delete => removed += 1,
                ChangeTag::Equal => {}
            }
        }
    }

    if added == 0 && removed == 0 {
        return None;
    }

    let mut output = String::new();
    output.push_str(&format!("  Changed lines: +{} / -{}\n", added, removed));

    let mut shown_lines = 0usize;
    for (idx, group) in groups.iter().take(3).enumerate() {
        let (old_start, old_count, new_start, new_count) = diff_group_header(group);
        output.push_str(&format!(
            "  Hunk {} @@ -{},{} +{},{} @@\n",
            idx + 1,
            old_start,
            old_count,
            new_start,
            new_count
        ));

        for op in group {
            for change in diff.iter_changes(op) {
                let prefix = match change.tag() {
                    ChangeTag::Delete => '-',
                    ChangeTag::Insert => '+',
                    ChangeTag::Equal => ' ',
                };
                output.push_str(&format!(
                    "    {}{}\n",
                    prefix,
                    summarize_compare_line(change.to_string().trim_end_matches('\n'))
                ));
                shown_lines += 1;
                if shown_lines >= 60 {
                    output.push_str("    ... diff preview truncated ...\n");
                    return Some(output);
                }
            }
        }
    }

    if groups.len() > 3 {
        output.push_str(&format!(
            "  ... {} more hunks omitted ...\n",
            groups.len() - 3
        ));
    }

    Some(output)
}

fn diff_group_header(group: &[DiffOp]) -> (usize, usize, usize, usize) {
    let first = group.first().expect("diff group should not be empty");
    let last = group.last().expect("diff group should not be empty");
    let old = first.old_range().start..last.old_range().end;
    let new = first.new_range().start..last.new_range().end;
    (
        old.start + 1,
        old.end.saturating_sub(old.start),
        new.start + 1,
        new.end.saturating_sub(new.start),
    )
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
        latest_transcript, parse_compare_targets, parse_date_range_filter, parse_list_filter,
        read_transcript_metadata, render_transcript_list, resolve_transcript_target,
        truncate_for_display, TranscriptListFilter, TranscriptMode, MAX_DISPLAY_CHARS,
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

        let listing = render_transcript_list(&dir, &TranscriptListFilter::default());
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

        let auto = filtered_transcript_entries(
            &dir,
            &TranscriptListFilter {
                mode: Some(TranscriptMode::Auto),
                ..Default::default()
            },
        );
        assert_eq!(auto.len(), 1);
        assert!(auto[0].ends_with("auto.md"));

        let manual_listing = render_transcript_list(
            &dir,
            &TranscriptListFilter {
                mode: Some(TranscriptMode::Manual),
                ..Default::default()
            },
        );
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

        let summary = filtered_transcript_entries(
            &dir,
            &TranscriptListFilter {
                require_summary: true,
                ..Default::default()
            },
        );
        assert_eq!(summary.len(), 1);
        assert!(summary[0].ends_with("with-summary.md"));

        let summary_listing = render_transcript_list(
            &dir,
            &TranscriptListFilter {
                require_summary: true,
                ..Default::default()
            },
        );
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

        let failed = filtered_transcript_entries(
            &dir,
            &TranscriptListFilter {
                require_failed: true,
                ..Default::default()
            },
        );
        assert_eq!(failed.len(), 1);
        assert!(failed[0].ends_with("failed.md"));

        let failed_listing = render_transcript_list(
            &dir,
            &TranscriptListFilter {
                require_failed: true,
                ..Default::default()
            },
        );
        assert!(failed_listing.contains("failed=2"));
        assert!(!failed_listing.contains("clean.md"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parse_list_filter_supports_date_ranges() {
        assert!(matches!(
            parse_list_filter("list 2026-01-01").unwrap(),
            TranscriptListFilter {
                date_range: Some(_),
                ..
            }
        ));
        assert!(matches!(
            parse_list_filter("list 2026-01-01..2026-01-03").unwrap(),
            TranscriptListFilter {
                date_range: Some(_),
                ..
            }
        ));
        assert!(matches!(
            parse_list_filter("list ..2026-01-03").unwrap(),
            TranscriptListFilter {
                date_range: Some(_),
                ..
            }
        ));
        assert!(matches!(
            parse_list_filter("list today").unwrap(),
            TranscriptListFilter {
                date_range: Some(_),
                ..
            }
        ));
        assert!(parse_list_filter("list 2026-01-03..2026-01-01").is_err());
        assert!(parse_list_filter("list nope").is_err());
    }

    #[test]
    fn parse_date_range_filter_supports_open_ranges() {
        let range = parse_date_range_filter("2026-01-01..").unwrap();
        assert_eq!(
            format!("{:?}", range),
            "DateRangeFilter { start: Some(2026-01-01), end: None }"
        );

        let range = parse_date_range_filter("..2026-01-03").unwrap();
        assert_eq!(
            format!("{:?}", range),
            "DateRangeFilter { start: None, end: Some(2026-01-03) }"
        );
    }

    #[test]
    fn filtered_transcript_entries_supports_date_range_filter() {
        let dir = std::env::temp_dir().join(format!(
            "yode-memory-command-date-filter-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("first.md"),
            "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-01 10:00:00\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("second.md"),
            "# Compaction Transcript\n\n- Mode: manual\n- Timestamp: 2026-01-03 11:00:00\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("third.md"),
            "# Compaction Transcript\n\n- Mode: manual\n- Timestamp: 2026-01-05 11:00:00\n",
        )
        .unwrap();

        let filter = parse_list_filter("list 2026-01-02..2026-01-04").unwrap();
        let filtered = filtered_transcript_entries(&dir, &filter);
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].ends_with("second.md"));

        let listing = render_transcript_list(&dir, &filter);
        assert!(listing.contains("(2026-01-02..2026-01-04)"));
        assert!(listing.contains("second.md"));
        assert!(!listing.contains("first.md"));
        assert!(!listing.contains("third.md"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parse_list_filter_supports_combined_flags() {
        let filter = parse_list_filter("list summary failed recent auto").unwrap();
        assert_eq!(
            filter,
            TranscriptListFilter {
                recent_limit: Some(5),
                mode: Some(TranscriptMode::Auto),
                require_summary: true,
                require_failed: true,
                date_range: None,
            }
        );
        assert!(parse_list_filter("list auto manual").is_err());
    }

    #[test]
    fn filtered_transcript_entries_supports_combined_filters() {
        let dir = std::env::temp_dir().join(format!(
            "yode-memory-command-combo-filter-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("match.md"),
            "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-03 10:00:00\n- Failed tool results: 2\n\n## Summary Anchor\n\n```text\nsummary\n```\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("no-failed.md"),
            "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-03 11:00:00\n- Failed tool results: 0\n\n## Summary Anchor\n\n```text\nsummary\n```\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("no-summary.md"),
            "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-03 12:00:00\n- Failed tool results: 1\n",
        )
        .unwrap();

        let filter = parse_list_filter("list auto summary failed").unwrap();
        let filtered = filtered_transcript_entries(&dir, &filter);
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].ends_with("match.md"));

        let listing = render_transcript_list(&dir, &filter);
        assert!(listing.contains("(auto summary failed)"));
        assert!(listing.contains("match.md"));
        assert!(!listing.contains("no-failed.md"));
        assert!(!listing.contains("no-summary.md"));

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
        assert!(output.contains("Content diff:"));
        assert!(output.contains("Changed lines:"));
        assert!(output.contains("Hunk 1"));
        assert!(output.contains("First difference:"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
