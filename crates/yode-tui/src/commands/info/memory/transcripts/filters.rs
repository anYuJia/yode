use chrono::{Duration, Local, NaiveDate, NaiveDateTime};
use std::fs;
use std::path::{Path, PathBuf};

use super::*;

pub(in crate::commands::info::memory) fn transcript_entries(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .collect()
}

pub(in crate::commands::info::memory) fn sorted_transcript_entries(dir: &Path) -> Vec<PathBuf> {
    let mut entries = transcript_entries(dir);
    entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    entries
}

pub(in crate::commands::info::memory) fn filtered_transcript_entries(
    dir: &Path,
    filter: &TranscriptListFilter,
) -> Vec<PathBuf> {
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

pub(in crate::commands::info::memory) fn latest_transcript(dir: &Path) -> Option<PathBuf> {
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

pub(in crate::commands::info::memory) fn parse_list_filter(
    args: &str,
) -> Result<TranscriptListFilter, String> {
    if args == "list" {
        return Ok(TranscriptListFilter::default());
    }

    let spec = args.strip_prefix("list ").ok_or_else(memory_list_usage)?.trim();
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

pub(in crate::commands::info::memory) fn parse_date_range_filter(
    spec: &str,
) -> Option<DateRangeFilter> {
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

fn memory_list_usage() -> String {
    "Usage: /memory list [recent] [auto|manual] [summary] [failed] [today|yesterday|YYYY-MM-DD|YYYY-MM-DD..YYYY-MM-DD|..YYYY-MM-DD|YYYY-MM-DD..]".to_string()
}

fn parse_iso_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

fn parse_transcript_date(timestamp: &str) -> Option<NaiveDate> {
    NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|ts| ts.date())
        .or_else(|| parse_iso_date(timestamp))
}
