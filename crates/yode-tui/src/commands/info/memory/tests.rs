use chrono::{Duration, Local};

use super::{
    build_transcript_compare_output, extract_summary_preview, filtered_transcript_entries,
    fold_transcript_preview, latest_transcript, memory_entry_age, parse_compare_args,
    parse_date_range_filter, parse_latest_compare_target, parse_list_filter, parse_memory_document,
    read_transcript_metadata, render_memory_file, render_transcript_list, render_transcript_picker,
    resolve_compare_target, resolve_transcript_target, run_long_session_benchmark,
    truncate_for_display, warm_resume_transcript_caches, CompareArgs, CompareOptions,
    TranscriptListFilter, TranscriptMode, MAX_DISPLAY_CHARS,
};
use crate::commands::CommandOutput;

mod compare;
mod document;
mod filters;
mod transcript;
