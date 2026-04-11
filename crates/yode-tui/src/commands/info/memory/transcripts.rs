mod metadata_runtime;

use chrono::{Duration, Local, NaiveDate, NaiveDateTime};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

use super::compare::{build_transcript_compare_output, CompareOptions};
use super::{MAX_DISPLAY_CHARS, TRANSCRIPT_PREVIEW_MESSAGE_HEAD_LINES, TRANSCRIPT_PREVIEW_TAIL_LINES};
pub(crate) use self::metadata_runtime::{
    run_long_session_benchmark, warm_resume_transcript_caches,
};
pub(super) use self::metadata_runtime::{
    extract_summary_preview, read_transcript_metadata, transcript_picker_summary_preview,
};

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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LongSessionBenchmarkReport {
    pub transcript_dir: PathBuf,
    pub transcript_count: usize,
    pub cold_latest_lookup_ms: u64,
    pub hot_latest_lookup_ms: u64,
    pub cold_failed_filter_ms: u64,
    pub hot_failed_filter_ms: u64,
    pub resume_warmup: ResumeTranscriptCacheWarmupStats,
    pub compare_pair: Option<(String, String)>,
    pub compare_ms: Option<u64>,
    pub compare_summary_only: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct TranscriptListFilter {
    pub recent_limit: Option<usize>,
    pub mode: Option<TranscriptMode>,
    pub require_summary: bool,
    pub require_failed: bool,
    pub date_range: Option<DateRangeFilter>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TranscriptMode {
    Auto,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DateRangeFilter {
    pub start: Option<NaiveDate>,
    pub end: Option<NaiveDate>,
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
    pub(super) fn label(&self) -> String {
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

#[derive(Debug, Clone, Default)]
pub(super) struct TranscriptMetadata {
    pub timestamp: Option<String>,
    pub mode: Option<String>,
    pub removed: Option<usize>,
    pub truncated: Option<usize>,
    pub failed_tool_results: Option<usize>,
    pub session_memory_path: Option<String>,
    pub files_read_summary: Option<String>,
    pub files_modified_summary: Option<String>,
    pub has_summary: bool,
}

pub(super) fn transcript_entries(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .collect()
}

pub(super) fn sorted_transcript_entries(dir: &Path) -> Vec<PathBuf> {
    let mut entries = transcript_entries(dir);
    entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    entries
}

pub(super) fn filtered_transcript_entries(dir: &Path, filter: &TranscriptListFilter) -> Vec<PathBuf> {
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

pub(super) fn latest_transcript(dir: &Path) -> Option<PathBuf> {
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

pub(super) fn parse_list_filter(args: &str) -> Result<TranscriptListFilter, String> {
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
            "recent" => filter.recent_limit = Some(5),
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
            "summary" => filter.require_summary = true,
            "failed" => filter.require_failed = true,
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

pub(super) fn resolve_transcript_target(dir: &Path, target: &str) -> Option<PathBuf> {
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

pub(super) fn resolve_compare_target(dir: &Path, target: &str) -> Option<PathBuf> {
    if let Some(path) = resolve_latest_alias(dir, target) {
        Some(path)
    } else {
        resolve_transcript_target(dir, target)
    }
}

pub(super) fn parse_latest_compare_target(args: &str) -> Option<&str> {
    let target = args.strip_prefix("latest compare ")?.trim();
    if target.is_empty() {
        None
    } else {
        Some(target)
    }
}

pub(super) fn describe_path(path: &Path) -> String {
    match fs::metadata(path) {
        Ok(meta) => format!("{} ({} bytes)", path.display(), meta.len()),
        Err(_) => format!("{} (missing)", path.display()),
    }
}

pub(super) fn truncate_for_display(content: &str) -> String {
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

pub(super) fn fold_transcript_preview(content: &str) -> String {
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

fn memory_list_usage() -> String {
    "Usage: /memory list [recent] [auto|manual] [summary] [failed] [today|yesterday|YYYY-MM-DD|YYYY-MM-DD..YYYY-MM-DD|..YYYY-MM-DD|YYYY-MM-DD..]".to_string()
}

pub(super) fn parse_date_range_filter(spec: &str) -> Option<DateRangeFilter> {
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

fn file_cache_stamp(path: &Path) -> Option<u64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

fn parse_transcript_date(timestamp: &str) -> Option<NaiveDate> {
    NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|ts| ts.date())
        .or_else(|| parse_iso_date(timestamp))
}
