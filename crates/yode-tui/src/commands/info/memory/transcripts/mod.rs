mod filters;
mod metadata_runtime;
mod targets;
mod types;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

#[cfg(test)]
pub(in crate::commands::info::memory) use self::filters::parse_date_range_filter;
pub(in crate::commands::info::memory) use self::filters::{
    filtered_transcript_entries, latest_transcript, parse_list_filter, sorted_transcript_entries,
    transcript_entries,
};
pub(in crate::commands::info::memory) use self::metadata_runtime::{
    extract_summary_preview, read_transcript_metadata, transcript_picker_summary_preview,
};
pub(crate) use self::metadata_runtime::{
    run_long_session_benchmark, warm_resume_transcript_caches,
};
pub(in crate::commands::info::memory) use self::targets::{
    describe_path, fold_transcript_preview, parse_latest_compare_target, resolve_compare_target,
    resolve_transcript_target, transcript_target_resolution_error, truncate_for_display,
};
pub(in crate::commands::info::memory) use self::types::{
    DateRangeFilter, TranscriptListFilter, TranscriptMetadata, TranscriptMode,
};
pub(crate) use self::types::{LongSessionBenchmarkReport, ResumeTranscriptCacheWarmupStats};

use super::compare::{build_transcript_compare_output, CompareOptions};
use super::{
    MAX_DISPLAY_CHARS, TRANSCRIPT_PREVIEW_MESSAGE_HEAD_LINES, TRANSCRIPT_PREVIEW_TAIL_LINES,
};

static TRANSCRIPT_META_CACHE: LazyLock<Mutex<HashMap<PathBuf, (u64, TranscriptMetadata)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static LATEST_TRANSCRIPT_CACHE: LazyLock<Mutex<HashMap<PathBuf, (u64, Option<PathBuf>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static TRANSCRIPT_CACHE_STATS: LazyLock<Mutex<TranscriptCacheStats>> =
    LazyLock::new(|| Mutex::new(TranscriptCacheStats::default()));

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TranscriptCacheStats {
    pub metadata_hits: u64,
    pub metadata_misses: u64,
    pub latest_hits: u64,
    pub latest_misses: u64,
    pub invalidations: u64,
    pub last_invalidation_reason: Option<String>,
}

pub(crate) fn transcript_cache_stats() -> TranscriptCacheStats {
    TRANSCRIPT_CACHE_STATS
        .lock()
        .map(|stats| stats.clone())
        .unwrap_or_default()
}

fn note_metadata_cache_hit() {
    if let Ok(mut stats) = TRANSCRIPT_CACHE_STATS.lock() {
        stats.metadata_hits = stats.metadata_hits.saturating_add(1);
    }
}

fn note_metadata_cache_miss() {
    if let Ok(mut stats) = TRANSCRIPT_CACHE_STATS.lock() {
        stats.metadata_misses = stats.metadata_misses.saturating_add(1);
    }
}

fn note_latest_cache_hit() {
    if let Ok(mut stats) = TRANSCRIPT_CACHE_STATS.lock() {
        stats.latest_hits = stats.latest_hits.saturating_add(1);
    }
}

fn note_latest_cache_miss() {
    if let Ok(mut stats) = TRANSCRIPT_CACHE_STATS.lock() {
        stats.latest_misses = stats.latest_misses.saturating_add(1);
    }
}

fn note_cache_invalidation(reason: impl Into<String>) {
    if let Ok(mut stats) = TRANSCRIPT_CACHE_STATS.lock() {
        stats.invalidations = stats.invalidations.saturating_add(1);
        stats.last_invalidation_reason = Some(reason.into());
    }
}

fn file_cache_stamp(path: &Path) -> Option<u64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}
