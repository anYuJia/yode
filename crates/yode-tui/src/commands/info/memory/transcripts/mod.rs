mod filters;
mod metadata_runtime;
mod targets;
mod types;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

pub(in crate::commands::info::memory) use self::filters::{
    filtered_transcript_entries, latest_transcript, parse_list_filter, sorted_transcript_entries,
    transcript_entries,
};
#[cfg(test)]
pub(in crate::commands::info::memory) use self::filters::parse_date_range_filter;
pub(in crate::commands::info::memory) use self::metadata_runtime::{
    extract_summary_preview, read_transcript_metadata, transcript_picker_summary_preview,
};
pub(in crate::commands::info::memory) use self::targets::{
    describe_path, fold_transcript_preview, parse_latest_compare_target, resolve_compare_target,
    resolve_transcript_target, truncate_for_display,
};
pub(crate) use self::metadata_runtime::{
    run_long_session_benchmark, warm_resume_transcript_caches,
};
pub(crate) use self::types::{LongSessionBenchmarkReport, ResumeTranscriptCacheWarmupStats};
pub(in crate::commands::info::memory) use self::types::{
    DateRangeFilter, TranscriptListFilter, TranscriptMetadata, TranscriptMode,
};

use super::compare::{build_transcript_compare_output, CompareOptions};
use super::{
    MAX_DISPLAY_CHARS, TRANSCRIPT_PREVIEW_MESSAGE_HEAD_LINES, TRANSCRIPT_PREVIEW_TAIL_LINES,
};

static TRANSCRIPT_META_CACHE: LazyLock<Mutex<HashMap<PathBuf, (u64, TranscriptMetadata)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static LATEST_TRANSCRIPT_CACHE: LazyLock<Mutex<HashMap<PathBuf, (u64, Option<PathBuf>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn file_cache_stamp(path: &Path) -> Option<u64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}
