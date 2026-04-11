use chrono::NaiveDate;
use std::path::PathBuf;

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
pub(in crate::commands::info::memory) struct TranscriptListFilter {
    pub recent_limit: Option<usize>,
    pub mode: Option<TranscriptMode>,
    pub require_summary: bool,
    pub require_failed: bool,
    pub date_range: Option<DateRangeFilter>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::commands::info::memory) enum TranscriptMode {
    Auto,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::commands::info::memory) struct DateRangeFilter {
    pub start: Option<NaiveDate>,
    pub end: Option<NaiveDate>,
}

impl DateRangeFilter {
    pub(in crate::commands::info::memory) fn single(date: NaiveDate) -> Self {
        Self {
            start: Some(date),
            end: Some(date),
        }
    }

    pub(in crate::commands::info::memory) fn label(&self) -> String {
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

    pub(in crate::commands::info::memory) fn contains(&self, date: NaiveDate) -> bool {
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
    pub(in crate::commands::info::memory) fn label(&self) -> String {
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
pub(in crate::commands::info::memory) struct TranscriptMetadata {
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
