use chrono::{Duration, Local, NaiveDate, NaiveDateTime};
use similar::{ChangeTag, DiffOp, TextDiff};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

const MAX_DISPLAY_CHARS: usize = 12_000;
const MAX_COMPARE_CONTENT_CHARS: usize = 200_000;
const TRANSCRIPT_PREVIEW_MESSAGE_HEAD_LINES: usize = 18;
const TRANSCRIPT_PREVIEW_TAIL_LINES: usize = 12;

static TRANSCRIPT_META_CACHE: LazyLock<Mutex<HashMap<PathBuf, (u64, TranscriptMetadata)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static LATEST_TRANSCRIPT_CACHE: LazyLock<Mutex<HashMap<PathBuf, (u64, Option<PathBuf>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ResumeTranscriptCacheWarmupStats {
    pub transcript_count: usize,
    pub metadata_entries_warmed: usize,
    pub latest_lookup_cached: bool,
    pub duration_ms: u64,
}

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
                    hint: "[live|session|latest|list [filters...]|compare <a> <b> [--no-diff|--hunks N|--lines N]|<index>|<file>]".to_string(),
                    completions: ArgCompletionSource::Static(vec![
                        "live".to_string(),
                        "session".to_string(),
                        "latest".to_string(),
                        "pick".to_string(),
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
                        "compare latest latest-1".to_string(),
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
            let compare = parse_compare_args(args).ok_or_else(|| {
                "Usage: /memory compare <a> <b> [--no-diff] [--hunks N] [--lines N]".to_string()
            })?;
            return render_transcript_compare(&transcripts_dir, &compare);
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
                ctx.session.resume_cache_warmup.as_ref(),
            ))),
            "live" => render_memory_file("Live session memory", &live_path),
            "session" => render_memory_file("Compaction memory", &session_path),
            "pick" => Ok(CommandOutput::Message(render_transcript_picker(
                &transcripts_dir,
            ))),
            _ if args.starts_with("latest compare ") => parse_latest_compare_target(args)
                .ok_or_else(|| "Usage: /memory latest compare <target>".to_string())
                .and_then(|target| {
                    render_transcript_compare(
                        &transcripts_dir,
                        &CompareArgs {
                            left_target: "latest".to_string(),
                            right_target: target.to_string(),
                            options: CompareOptions::default(),
                        },
                    )
                }),
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
                render_transcript_file(&transcript)
            }
        }
    }
}

fn render_memory_status(
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
    let resume_warmup_line = resume_warmup
        .map(|stats| {
            format!(
                "\n  Resume warmup:    {} transcripts / {} metadata / latest={} / {} ms",
                stats.transcript_count,
                stats.metadata_entries_warmed,
                if stats.latest_lookup_cached { "yes" } else { "no" },
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

fn render_transcript_file(path: &Path) -> CommandResult {
    let content =
        fs::read_to_string(path).map_err(|_| format!("Transcript not found: {}", path.display()))?;
    Ok(CommandOutput::Message(format!(
        "Transcript\nPath: {}\n\n{}",
        path.display(),
        fold_transcript_preview(&content)
    )))
}

fn render_memory_file(label: &str, path: &Path) -> CommandResult {
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
                    if section.items.is_empty() {
                        "none".to_string()
                    } else {
                        section.items.join(" | ")
                    }
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

fn render_latest_transcript(path: &Path) -> CommandResult {
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

fn render_transcript_picker(dir: &Path) -> String {
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
    }
    output.push_str("\nUse /memory <index> to open one, or /memory compare <a> <b> to diff two.");
    output
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompareArgs {
    left_target: String,
    right_target: String,
    options: CompareOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompareOptions {
    diff_enabled: bool,
    max_hunks: usize,
    max_lines: usize,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            diff_enabled: true,
            max_hunks: 3,
            max_lines: 60,
        }
    }
}

fn render_transcript_compare(dir: &Path, compare: &CompareArgs) -> CommandResult {
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
    let stamp = file_cache_stamp(dir)?;
    if let Ok(cache) = LATEST_TRANSCRIPT_CACHE.lock() {
        if let Some((cached_stamp, cached_path)) = cache.get(dir) {
            if *cached_stamp == stamp {
                return cached_path.clone();
            }
        }
    }

    let latest = sorted_transcript_entries(dir).into_iter().next();
    if let Ok(mut cache) = LATEST_TRANSCRIPT_CACHE.lock() {
        cache.insert(dir.to_path_buf(), (stamp, latest.clone()));
    }
    latest
}

pub(crate) fn warm_resume_transcript_caches(
    project_root: &Path,
) -> ResumeTranscriptCacheWarmupStats {
    let started_at = Instant::now();
    let transcripts_dir = project_root.join(".yode").join("transcripts");
    let entries = sorted_transcript_entries(&transcripts_dir);
    let transcript_count = entries.len();
    let latest = entries.first().cloned();

    if let Some(stamp) = file_cache_stamp(&transcripts_dir) {
        if let Ok(mut cache) = LATEST_TRANSCRIPT_CACHE.lock() {
            cache.insert(transcripts_dir.clone(), (stamp, latest.clone()));
        }
    }

    let mut metadata_entries_warmed = 0;
    for path in &entries {
        if read_transcript_metadata(path).is_some() {
            metadata_entries_warmed += 1;
        }
    }

    ResumeTranscriptCacheWarmupStats {
        transcript_count,
        metadata_entries_warmed,
        latest_lookup_cached: latest.is_some(),
        duration_ms: started_at.elapsed().as_millis() as u64,
    }
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
    if let Some(path) = resolve_latest_alias(dir, target) {
        return Some(path);
    }
    if let Ok(index) = target.parse::<usize>() {
        if index == 0 {
            return None;
        }
        return entries.get(index - 1).cloned();
    }

    entries
        .into_iter()
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name == target)
                .unwrap_or(false)
                || path.display().to_string() == target
        })
        .or_else(|| resolve_transcript_target_fuzzy(dir, target))
}

fn resolve_transcript_target_fuzzy(dir: &Path, target: &str) -> Option<PathBuf> {
    let target = target.trim();
    if target.is_empty() {
        return None;
    }

    let entries = sorted_transcript_entries(dir);
    let exact_basename = entries
        .iter()
        .filter(|path| {
            path.file_stem()
                .and_then(|name| name.to_str())
                .map(|name| name == target)
                .unwrap_or(false)
        })
        .cloned()
        .collect::<Vec<_>>();
    if exact_basename.len() == 1 {
        return exact_basename.into_iter().next();
    }

    let prefix_matches = entries
        .iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with(target))
                .unwrap_or(false)
        })
        .cloned()
        .collect::<Vec<_>>();
    if prefix_matches.len() == 1 {
        return prefix_matches.into_iter().next();
    }

    let contains_matches = entries
        .iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.contains(target))
                .unwrap_or(false)
        })
        .cloned()
        .collect::<Vec<_>>();
    if contains_matches.len() == 1 {
        return contains_matches.into_iter().next();
    }

    None
}

fn resolve_compare_target(dir: &Path, target: &str) -> Option<PathBuf> {
    if let Some(path) = resolve_latest_alias(dir, target) {
        Some(path)
    } else {
        resolve_transcript_target(dir, target)
    }
}

fn resolve_latest_alias(dir: &Path, target: &str) -> Option<PathBuf> {
    if target == "latest" {
        return latest_transcript(dir);
    }

    let suffix = target.strip_prefix("latest-")?;
    let offset = suffix.parse::<usize>().ok()?;
    if offset == 0 {
        return latest_transcript(dir);
    }

    sorted_transcript_entries(dir).into_iter().nth(offset)
}

fn parse_compare_args(args: &str) -> Option<CompareArgs> {
    let rest = args.strip_prefix("compare ")?;
    let tokens = rest.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 2 {
        return None;
    }
    let mut compare = CompareArgs {
        left_target: tokens[0].to_string(),
        right_target: tokens[1].to_string(),
        options: CompareOptions::default(),
    };

    let mut idx = 2usize;
    while idx < tokens.len() {
        match tokens[idx] {
            "--no-diff" => {
                compare.options.diff_enabled = false;
                idx += 1;
            }
            "--hunks" => {
                let value = tokens.get(idx + 1)?.parse::<usize>().ok()?;
                if value == 0 {
                    return None;
                }
                compare.options.max_hunks = value;
                idx += 2;
            }
            "--lines" => {
                let value = tokens.get(idx + 1)?.parse::<usize>().ok()?;
                if value == 0 {
                    return None;
                }
                compare.options.max_lines = value;
                idx += 2;
            }
            _ => return None,
        }
    }

    Some(compare)
}

fn parse_latest_compare_target(args: &str) -> Option<&str> {
    let target = args.strip_prefix("latest compare ")?.trim();
    if target.is_empty() {
        None
    } else {
        Some(target)
    }
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

    let notice = format!(
        "[Truncated for display at {} chars. Scroll for earlier content if your terminal keeps history, or open the file path directly.]",
        MAX_DISPLAY_CHARS
    );
    let budget = MAX_DISPLAY_CHARS.saturating_sub(notice.chars().count() + 2);
    let truncated = content.chars().take(budget).collect::<String>();
    format!("{}\n\n{}", truncated, notice)
}

fn fold_transcript_preview(content: &str) -> String {
    let lines = content.lines().collect::<Vec<_>>();
    let Some(messages_idx) = lines.iter().position(|line| *line == "## Messages") else {
        return truncate_for_display(content);
    };

    if content.chars().count() <= MAX_DISPLAY_CHARS
        && lines.len()
            <= messages_idx + TRANSCRIPT_PREVIEW_MESSAGE_HEAD_LINES + TRANSCRIPT_PREVIEW_TAIL_LINES
    {
        return content.to_string();
    }

    let tail_start = lines
        .len()
        .saturating_sub(TRANSCRIPT_PREVIEW_TAIL_LINES)
        .max(messages_idx + 1);
    let message_preview_end =
        (messages_idx + TRANSCRIPT_PREVIEW_MESSAGE_HEAD_LINES).min(tail_start);

    let mut output = String::new();
    let pre_messages = lines[..messages_idx].join("\n");
    if !pre_messages.trim().is_empty() {
        output.push_str(pre_messages.trim_end());
        output.push_str("\n\n");
    }

    output.push_str("## Messages Preview\n\n");
    output.push_str(&lines[messages_idx..message_preview_end].join("\n"));

    if tail_start > message_preview_end {
        output.push_str(&format!(
            "\n\n... [transcript preview folded: {} middle lines omitted] ...\n\n",
            tail_start - message_preview_end
        ));
        output.push_str(&lines[tail_start..].join("\n"));
    }

    if output.chars().count() > MAX_DISPLAY_CHARS {
        truncate_for_display(&output)
    } else {
        output
    }
}

fn file_cache_stamp(path: &Path) -> Option<u64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

#[derive(Debug, Clone, Default)]
struct TranscriptMetadata {
    timestamp: Option<String>,
    mode: Option<String>,
    removed: Option<usize>,
    truncated: Option<usize>,
    failed_tool_results: Option<usize>,
    session_memory_path: Option<String>,
    files_read_summary: Option<String>,
    files_modified_summary: Option<String>,
    has_summary: bool,
}

fn read_transcript_metadata(path: &Path) -> Option<TranscriptMetadata> {
    let stamp = file_cache_stamp(path)?;
    if let Ok(cache) = TRANSCRIPT_META_CACHE.lock() {
        if let Some((cached_stamp, cached_meta)) = cache.get(path) {
            if *cached_stamp == stamp {
                return Some(cached_meta.clone());
            }
        }
    }

    let content = fs::read_to_string(path).ok()?;
    let mut meta = TranscriptMetadata::default();

    for line in content.lines().take(14) {
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
        } else if let Some(value) = line.strip_prefix("- Session memory path: ") {
            meta.session_memory_path = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("- Files read: ") {
            meta.files_read_summary = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("- Files modified: ") {
            meta.files_modified_summary = Some(value.to_string());
        }
    }

    meta.has_summary = content.contains("## Summary Anchor");
    if let Ok(mut cache) = TRANSCRIPT_META_CACHE.lock() {
        cache.insert(path.to_path_buf(), (stamp, meta.clone()));
    }

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
    options: &CompareOptions,
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
    let compare_too_large = left_chars.saturating_add(right_chars) > MAX_COMPARE_CONTENT_CHARS;

    let mut output = String::new();
    output.push_str("Transcript comparison\n");
    output.push_str(&format!("A: {}\n", left_path.display()));
    output.push_str(&format!("B: {}\n", right_path.display()));
    output.push_str(&format!(
        "Status: {}\n\n",
        if identical { "identical" } else { "different" }
    ));
    output.push_str(&format!(
        "Diff window: hunks={} lines={}\n\n",
        options.max_hunks, options.max_lines
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
        "Session memory path",
        left_meta.session_memory_path.as_deref().unwrap_or("none"),
        right_meta.session_memory_path.as_deref().unwrap_or("none"),
    ));
    output.push_str(&format_compare_field(
        "Files read",
        left_meta.files_read_summary.as_deref().unwrap_or("none"),
        right_meta.files_read_summary.as_deref().unwrap_or("none"),
    ));
    output.push_str(&format_compare_field(
        "Files modified",
        left_meta
            .files_modified_summary
            .as_deref()
            .unwrap_or("none"),
        right_meta
            .files_modified_summary
            .as_deref()
            .unwrap_or("none"),
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

    output.push_str("\nSection summary:\n");
    output.push_str(&build_section_summary(left_content, right_content));

    if compare_too_large {
        output.push_str("\nContent diff:\n");
        output.push_str(&format!(
            "  skipped: content too large for interactive diff preview ({} chars > {}). Use --no-diff, narrower targets, or inspect one transcript directly.\n",
            left_chars + right_chars,
            MAX_COMPARE_CONTENT_CHARS
        ));
    } else if options.diff_enabled {
        if let Some(diff_preview) = build_diff_preview(left_content, right_content, options) {
            output.push_str("\nContent diff:\n");
            output.push_str(&diff_preview);
        }
    } else {
        output.push_str("\nContent diff:\n");
        output.push_str("  disabled by flag\n");
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

fn build_diff_preview(left: &str, right: &str, options: &CompareOptions) -> Option<String> {
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
    for (idx, group) in groups.iter().take(options.max_hunks).enumerate() {
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
                if shown_lines >= options.max_lines {
                    output.push_str(
                        "    ... diff preview truncated ... use --lines N or --hunks N to expand ...\n",
                    );
                    return Some(output);
                }
            }
        }
    }

    if groups.len() > options.max_hunks {
        output.push_str(&format!(
            "  ... {} more hunks omitted ... use --hunks N to expand ...\n",
            groups.len() - options.max_hunks
        ));
    }

    Some(output)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TranscriptSectionStats {
    summary_anchor_lines: usize,
    message_lines: usize,
    role_counts: std::collections::BTreeMap<String, usize>,
}

fn build_section_summary(left: &str, right: &str) -> String {
    let left_stats = transcript_section_stats(left);
    let right_stats = transcript_section_stats(right);

    let mut lines = Vec::new();
    lines.push(format!(
        "  Summary Anchor lines: {} -> {}",
        left_stats.summary_anchor_lines, right_stats.summary_anchor_lines
    ));
    lines.push(format!(
        "  Messages lines:       {} -> {}",
        left_stats.message_lines, right_stats.message_lines
    ));

    let mut roles = left_stats
        .role_counts
        .keys()
        .chain(right_stats.role_counts.keys())
        .cloned()
        .collect::<Vec<_>>();
    roles.sort();
    roles.dedup();
    for role in roles {
        let left_count = left_stats.role_counts.get(&role).copied().unwrap_or(0);
        let right_count = right_stats.role_counts.get(&role).copied().unwrap_or(0);
        lines.push(format!(
            "  {} blocks: {} -> {}",
            role, left_count, right_count
        ));
    }

    format!("{}\n", lines.join("\n"))
}

fn transcript_section_stats(content: &str) -> TranscriptSectionStats {
    let mut stats = TranscriptSectionStats {
        summary_anchor_lines: 0,
        message_lines: 0,
        role_counts: std::collections::BTreeMap::new(),
    };

    let mut current_section: Option<&str> = None;
    for line in content.lines() {
        if let Some(section) = line.strip_prefix("## ") {
            current_section = Some(section.trim());
            continue;
        }

        match current_section {
            Some("Summary Anchor") if !line.trim().is_empty() => {
                stats.summary_anchor_lines += 1;
            }
            Some("Messages") if !line.trim().is_empty() => {
                stats.message_lines += 1;
                if let Some(role) = line.strip_prefix("### ") {
                    *stats
                        .role_counts
                        .entry(role.trim().to_string())
                        .or_insert(0) += 1;
                }
            }
            _ => {}
        }
    }

    stats
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct MemoryDocumentView {
    entries: Vec<MemoryEntryView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MemoryEntryView {
    timestamp: Option<String>,
    session_id: Option<String>,
    sections: Vec<MemorySectionView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MemorySectionView {
    title: String,
    items: Vec<String>,
}

fn parse_memory_document(content: &str) -> Option<MemoryDocumentView> {
    let mut entries = Vec::new();
    let mut current_entry: Option<MemoryEntryView> = None;
    let mut current_section: Option<MemorySectionView> = None;

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            if rest.contains(" session ") {
                flush_memory_section(&mut current_entry, &mut current_section);
                if let Some(entry) = current_entry.take() {
                    entries.push(entry);
                }
                current_entry = Some(parse_memory_entry_header(rest));
            }
            continue;
        }

        let Some(entry) = current_entry.as_mut() else {
            continue;
        };

        if let Some(title) = line.strip_prefix("### ") {
            flush_memory_section(&mut current_entry, &mut current_section);
            current_section = Some(MemorySectionView {
                title: title.trim().to_string(),
                items: Vec::new(),
            });
            continue;
        }

        if let Some(section) = current_section.as_mut() {
            if let Some(item) = line.strip_prefix("- ") {
                section.items.push(item.trim().to_string());
            } else if !line.trim().is_empty()
                && section.title == "Session Stats"
                && !line.starts_with("```")
            {
                section.items.push(line.trim().to_string());
            }
        } else if !line.trim().is_empty() && !line.starts_with('#') {
            let _ = entry;
        }
    }

    flush_memory_section(&mut current_entry, &mut current_section);
    if let Some(entry) = current_entry.take() {
        entries.push(entry);
    }

    if entries.is_empty() || !entries.iter().any(|entry| !entry.sections.is_empty()) {
        None
    } else {
        Some(MemoryDocumentView { entries })
    }
}

fn parse_memory_entry_header(header: &str) -> MemoryEntryView {
    let (timestamp, session_id) =
        if let Some((timestamp, session_id)) = header.split_once(" session ") {
            (
                Some(timestamp.trim().to_string()),
                Some(session_id.trim().to_string()),
            )
        } else {
            (Some(header.trim().to_string()), None)
        };

    MemoryEntryView {
        timestamp,
        session_id,
        sections: Vec::new(),
    }
}

fn flush_memory_section(
    entry: &mut Option<MemoryEntryView>,
    section: &mut Option<MemorySectionView>,
) {
    if let (Some(entry), Some(section)) = (entry.as_mut(), section.take()) {
        entry.sections.push(section);
    }
}

fn memory_entry_age(timestamp: Option<&str>) -> String {
    let Some(timestamp) = timestamp else {
        return "unknown".to_string();
    };
    let Some(dt) = NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S").ok() else {
        return timestamp.to_string();
    };

    let now = Local::now().naive_local();
    let delta = now - dt;
    let hours = delta.num_hours();
    if hours < 0 {
        return "from the future".to_string();
    }
    if hours < 1 {
        return "less than 1 hour old".to_string();
    }
    if hours < 24 {
        return format!("{} hours old", hours);
    }

    let days = delta.num_days();
    if days == 1 {
        "1 day old".to_string()
    } else {
        format!("{} days old", days)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Local};

    use super::{
        build_transcript_compare_output, extract_summary_preview, filtered_transcript_entries,
        fold_transcript_preview, latest_transcript, memory_entry_age, parse_compare_args,
        parse_date_range_filter, parse_latest_compare_target, parse_list_filter,
        parse_memory_document, read_transcript_metadata, render_memory_file,
        render_transcript_list, resolve_compare_target, resolve_transcript_target,
        truncate_for_display, warm_resume_transcript_caches, CompareArgs, CompareOptions,
        TranscriptListFilter, TranscriptMode, MAX_DISPLAY_CHARS,
    };
    use crate::commands::CommandOutput;

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
    fn fold_transcript_preview_preserves_summary_and_folds_messages() {
        let mut content = String::from(
            "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-01 10:00:00\n\n## Summary Anchor\n\n```text\nsummary line\n```\n\n## Messages\n",
        );
        for i in 0..80 {
            content.push_str(&format!("### Message {}\n\n```text\nline {}\n```\n\n", i, i));
        }

        let folded = fold_transcript_preview(&content);
        assert!(folded.contains("## Summary Anchor"));
        assert!(folded.contains("## Messages Preview"));
        assert!(folded.contains("transcript preview folded"));
        assert!(folded.contains("Message 79"));
    }

    #[test]
    fn warm_resume_transcript_caches_reports_warmed_entries() {
        let project_root = std::env::temp_dir().join(format!(
            "yode-memory-warmup-{}",
            uuid::Uuid::new_v4()
        ));
        let transcript_dir = project_root.join(".yode").join("transcripts");
        std::fs::create_dir_all(&transcript_dir).unwrap();
        std::fs::write(
            transcript_dir.join("aaa-compact-20260101.md"),
            "# Compaction Transcript\n\n- Mode: auto\n",
        )
        .unwrap();
        std::fs::write(
            transcript_dir.join("bbb-compact-20260102.md"),
            "# Compaction Transcript\n\n- Mode: manual\n",
        )
        .unwrap();

        let stats = warm_resume_transcript_caches(&project_root);
        assert_eq!(stats.transcript_count, 2);
        assert_eq!(stats.metadata_entries_warmed, 2);
        assert!(stats.latest_lookup_cached);

        std::fs::remove_dir_all(&project_root).ok();
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
            "# Compaction Transcript\n\n- Session: abc\n- Mode: manual\n- Timestamp: 2026-01-01 10:00:00\n- Removed messages: 7\n- Tool results truncated: 2\n- Failed tool results: 1\n- Session memory path: .yode/memory/session.md\n- Files read: src/lib.rs (120 lines)\n- Files modified: src/main.rs\n\n## Summary Anchor\n",
        )
        .unwrap();

        let meta = read_transcript_metadata(&path).unwrap();
        assert_eq!(meta.mode.as_deref(), Some("manual"));
        assert_eq!(meta.timestamp.as_deref(), Some("2026-01-01 10:00:00"));
        assert_eq!(meta.removed, Some(7));
        assert_eq!(meta.truncated, Some(2));
        assert_eq!(meta.failed_tool_results, Some(1));
        assert_eq!(
            meta.session_memory_path.as_deref(),
            Some(".yode/memory/session.md")
        );
        assert_eq!(
            meta.files_read_summary.as_deref(),
            Some("src/lib.rs (120 lines)")
        );
        assert_eq!(meta.files_modified_summary.as_deref(), Some("src/main.rs"));
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
    fn parse_compare_args_accepts_two_values_and_flags() {
        assert_eq!(
            parse_compare_args("compare 1 2"),
            Some(CompareArgs {
                left_target: "1".to_string(),
                right_target: "2".to_string(),
                options: CompareOptions::default(),
            })
        );
        assert_eq!(
            parse_compare_args("compare latest sample.md --hunks 2 --lines 20"),
            Some(CompareArgs {
                left_target: "latest".to_string(),
                right_target: "sample.md".to_string(),
                options: CompareOptions {
                    diff_enabled: true,
                    max_hunks: 2,
                    max_lines: 20,
                },
            })
        );
        assert_eq!(
            parse_compare_args("compare latest latest-1 --no-diff"),
            Some(CompareArgs {
                left_target: "latest".to_string(),
                right_target: "latest-1".to_string(),
                options: CompareOptions {
                    diff_enabled: false,
                    ..CompareOptions::default()
                },
            })
        );
        assert_eq!(parse_compare_args("compare 1"), None);
        assert_eq!(parse_compare_args("list compare 1 2"), None);
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

        let output = build_transcript_compare_output(
            &left_path,
            left,
            &right_path,
            right,
            &CompareOptions::default(),
        );
        assert!(output.contains("Status: different"));
        assert!(output.contains("Mode               auto -> manual"));
        assert!(output.contains("Failed tool results 1 -> 0"));
        assert!(output.contains("A: Left summary"));
        assert!(output.contains("B: Right summary"));
        assert!(output.contains("Section summary:"));
        assert!(output.contains("User blocks:"));
        assert!(output.contains("Content diff:"));
        assert!(output.contains("Changed lines:"));
        assert!(output.contains("Hunk 1"));
        assert!(output.contains("First difference:"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn build_transcript_compare_output_respects_no_diff_flag() {
        let dir = std::env::temp_dir().join(format!(
            "yode-memory-command-compare-nodiff-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let left_path = dir.join("left.md");
        let right_path = dir.join("right.md");
        let left = "# Compaction Transcript\n\n## Summary Anchor\n\n```text\nLeft summary\n```\n\n## Messages\n\n### Assistant\n\n```text\nhello\n```\n";
        let right = "# Compaction Transcript\n\n## Summary Anchor\n\n```text\nRight summary\n```\n\n## Messages\n\n### Assistant\n\n```text\nworld\n```\n";
        std::fs::write(&left_path, left).unwrap();
        std::fs::write(&right_path, right).unwrap();

        let output = build_transcript_compare_output(
            &left_path,
            left,
            &right_path,
            right,
            &CompareOptions {
                diff_enabled: false,
                ..CompareOptions::default()
            },
        );
        assert!(output.contains("Content diff:\n  disabled by flag"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_compare_target_supports_latest_alias_offsets() {
        let dir = std::env::temp_dir().join(format!(
            "yode-memory-command-latest-alias-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("aaa-compact-20240101.md"), "old").unwrap();
        std::fs::write(dir.join("bbb-compact-20250101.md"), "mid").unwrap();
        std::fs::write(dir.join("ccc-compact-20260101.md"), "new").unwrap();

        let latest = resolve_compare_target(&dir, "latest").unwrap();
        assert!(latest.ends_with("ccc-compact-20260101.md"));

        let previous = resolve_compare_target(&dir, "latest-1").unwrap();
        assert!(previous.ends_with("bbb-compact-20250101.md"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parse_memory_document_reads_structured_sections() {
        let content = "# Session Snapshot\n\nYode refreshes this file automatically.\n\n## 2026-04-09 10:00:00 session abc12345\n\n### Goals\n\n- Goal one\n\n### Findings\n\n- Finding one\n\n### Decisions\n\n- Decision one\n\n### Files\n\n- Read: src/lib.rs\n\n### Open Questions\n\n- Question one\n\n### Freshness\n\n- Generated at: 2026-04-09 10:00:00\n\n### Confidence\n\n- High\n";
        let parsed = parse_memory_document(content).unwrap();
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(parsed.entries[0].session_id.as_deref(), Some("abc12345"));
        assert_eq!(parsed.entries[0].sections[0].title, "Goals");
        assert_eq!(parsed.entries[0].sections[0].items[0], "Goal one");
    }

    #[test]
    fn memory_entry_age_formats_recent_entries() {
        let now = Local::now().naive_local();
        let ts = (now - Duration::hours(3))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        assert_eq!(memory_entry_age(Some(&ts)), "3 hours old");
    }

    #[test]
    fn render_memory_file_prefers_structured_view() {
        let dir = std::env::temp_dir().join(format!(
            "yode-memory-command-structured-file-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.live.md");
        std::fs::write(
            &path,
            "# Session Snapshot\n\nYode refreshes this file during the session to preserve recent context between compactions.\n\n## 2026-04-09 10:00:00 session abc12345\n\n### Goals\n\n- Goal one\n\n### Findings\n\n- Finding one\n\n### Decisions\n\n- Decision one\n\n### Files\n\n- Read: src/lib.rs\n\n### Open Questions\n\n- Question one\n\n### Freshness\n\n- Generated at: 2026-04-09 10:00:00\n\n### Confidence\n\n- High\n",
        )
        .unwrap();

        let rendered = render_memory_file("Live session memory", &path).unwrap();
        let CommandOutput::Message(rendered) = rendered else {
            panic!("expected message output");
        };
        assert!(rendered.contains("Schema: structured-v1"));
        assert!(rendered.contains("Structured view:"));
        assert!(rendered.contains("Goals (1): Goal one"));
        assert!(rendered.contains("Freshness (1): Generated at: 2026-04-09 10:00:00"));
        assert!(rendered.contains("Raw markdown:"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn render_memory_file_falls_back_for_legacy_content() {
        let dir = std::env::temp_dir().join(format!(
            "yode-memory-command-legacy-file-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.md");
        std::fs::write(&path, "# Session Memory\n\nSummary:\nlegacy content\n").unwrap();

        let rendered = render_memory_file("Compaction memory", &path).unwrap();
        let CommandOutput::Message(rendered) = rendered else {
            panic!("expected message output");
        };
        assert!(!rendered.contains("Schema: structured-v1"));
        assert!(rendered.contains("legacy content"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_transcript_target_supports_unique_fuzzy_aliases() {
        let dir = std::env::temp_dir().join(format!(
            "yode-memory-command-fuzzy-target-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("abc12345-compact-20240101.md"), "old").unwrap();
        std::fs::write(dir.join("def67890-compact-20250101.md"), "new").unwrap();

        let fuzzy = resolve_transcript_target(&dir, "def67890-compact").unwrap();
        assert!(fuzzy.ends_with("def67890-compact-20250101.md"));

        let latest_alias = resolve_transcript_target(&dir, "latest-1").unwrap();
        assert!(latest_alias.ends_with("abc12345-compact-20240101.md"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parse_latest_compare_target_accepts_shortcut() {
        assert_eq!(parse_latest_compare_target("latest compare 2"), Some("2"));
        assert_eq!(
            parse_latest_compare_target("latest compare latest-1"),
            Some("latest-1")
        );
        assert_eq!(parse_latest_compare_target("latest compare "), None);
    }
}
