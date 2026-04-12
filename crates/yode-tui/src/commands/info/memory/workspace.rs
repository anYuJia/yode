use std::path::Path;

use yode_core::engine::EngineRuntimeState;

use super::transcripts::{describe_path, TranscriptMetadata};

pub(in crate::commands::info::memory) fn transcript_jump_target_summary(dir: &Path) -> String {
    let entries = super::transcripts::sorted_transcript_entries(dir);
    let latest = entries.first();
    let latest_prev = entries.get(1);
    format!(
        "Jump targets:\n  latest:   {}\n  latest-1: {}\n  picker:   /memory pick\n  compare:  /memory compare latest latest-1",
        latest
            .and_then(|path| path.file_name().and_then(|name| name.to_str()))
            .unwrap_or("none"),
        latest_prev
            .and_then(|path| path.file_name().and_then(|name| name.to_str()))
            .unwrap_or("none"),
    )
}

pub(in crate::commands::info::memory) fn transcript_metadata_panel(
    path: &Path,
    meta: &TranscriptMetadata,
    summary_preview: &str,
) -> String {
    format!(
        "Metadata panel:\n  Path:            {}\n  Timestamp:       {}\n  Mode:            {}\n  Removed:         {}\n  Truncated:       {}\n  Failed tools:    {}\n  Session memory:  {}\n  Files read:      {}\n  Files modified:  {}\n  Summary anchor:  {}\n  Summary preview: {}",
        describe_path(path),
        meta.timestamp.as_deref().unwrap_or("unknown"),
        meta.mode.as_deref().unwrap_or("unknown"),
        meta.removed.unwrap_or(0),
        meta.truncated.unwrap_or(0),
        meta.failed_tool_results.unwrap_or(0),
        meta.session_memory_path.as_deref().unwrap_or("none"),
        meta.files_read_summary.as_deref().unwrap_or("none"),
        meta.files_modified_summary.as_deref().unwrap_or("none"),
        if meta.has_summary { "yes" } else { "no" },
        if summary_preview.is_empty() {
            "none".to_string()
        } else {
            summary_preview.to_string()
        },
    )
}

pub(in crate::commands::info::memory) fn transcript_timeline_anchor_panel(
    path: &Path,
    meta: &TranscriptMetadata,
    runtime: Option<&EngineRuntimeState>,
) -> String {
    let matches_runtime = runtime
        .and_then(|state| state.last_compaction_transcript_path.as_deref())
        .map(|runtime_path| runtime_path == path.display().to_string())
        .unwrap_or(false);
    let runtime_summary = runtime.map(|state| {
        format!(
            "  Runtime compact: {} / {}\n  Runtime session memory: {}\n  Runtime turn artifact: {}",
            state.last_compaction_mode.as_deref().unwrap_or("none"),
            state.last_compaction_at.as_deref().unwrap_or("none"),
            state
                .last_compaction_session_memory_path
                .as_deref()
                .unwrap_or("none"),
            state.last_turn_artifact_path.as_deref().unwrap_or("none"),
        )
    });

    format!(
        "Timeline anchors:\n  Matches runtime latest transcript: {}\n  Transcript timestamp: {}\n  Transcript session memory: {}{}",
        if matches_runtime { "yes" } else { "no" },
        meta.timestamp.as_deref().unwrap_or("unknown"),
        meta.session_memory_path.as_deref().unwrap_or("none"),
        runtime_summary
            .map(|summary| format!("\n{}", summary))
            .unwrap_or_default(),
    )
}

pub(in crate::commands::info::memory) fn transcript_search_result_summary(
    index: usize,
    path: &Path,
    meta: &TranscriptMetadata,
    preview: Option<&str>,
) -> String {
    format!(
        "  {:>2}. {} | mode={} | failed={} | summary={} | {}\n      preview: {}",
        index,
        meta.timestamp.as_deref().unwrap_or("unknown time"),
        meta.mode.as_deref().unwrap_or("unknown"),
        meta.failed_tool_results.unwrap_or(0),
        if meta.has_summary { "yes" } else { "no" },
        path.display(),
        preview.unwrap_or("none")
    )
}

pub(in crate::commands::info::memory) fn diff_inspector_header(
    left_path: &Path,
    right_path: &Path,
    status: &str,
    max_hunks: usize,
    max_lines: usize,
) -> String {
    format!(
        "Diff inspector\n  A: {}\n  B: {}\n  Status: {}\n  Window: hunks={} lines={}",
        left_path.display(),
        right_path.display(),
        status,
        max_hunks,
        max_lines
    )
}

#[cfg(test)]
mod tests {
    use super::{
        diff_inspector_header, transcript_jump_target_summary, transcript_metadata_panel,
        transcript_search_result_summary, transcript_timeline_anchor_panel,
    };
    use crate::commands::info::memory::transcripts::TranscriptMetadata;

    #[test]
    fn metadata_panel_formats_core_fields() {
        let path = std::path::Path::new("/tmp/a.md");
        let meta = TranscriptMetadata {
            timestamp: Some("2026-01-01 00:00:00".to_string()),
            mode: Some("auto".to_string()),
            removed: Some(5),
            truncated: Some(1),
            failed_tool_results: Some(0),
            session_memory_path: Some("/tmp/memory.md".to_string()),
            files_read_summary: Some("src/lib.rs".to_string()),
            files_modified_summary: Some("src/main.rs".to_string()),
            has_summary: true,
        };
        let panel = transcript_metadata_panel(path, &meta, "summary line");
        assert!(panel.contains("Path:"));
        assert!(panel.contains("summary line"));
    }

    #[test]
    fn jump_target_summary_mentions_latest_aliases() {
        let dir = std::env::temp_dir().join(format!("yode-jump-targets-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.md"), "x").unwrap();
        std::fs::write(dir.join("b.md"), "x").unwrap();
        let summary = transcript_jump_target_summary(&dir);
        assert!(summary.contains("latest"));
        assert!(summary.contains("latest-1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn search_result_summary_includes_preview() {
        let path = std::path::Path::new("/tmp/a.md");
        let meta = TranscriptMetadata {
            timestamp: Some("2026-01-01 00:00:00".to_string()),
            mode: Some("auto".to_string()),
            failed_tool_results: Some(1),
            has_summary: true,
            ..TranscriptMetadata::default()
        };
        let line = transcript_search_result_summary(2, path, &meta, Some("preview"));
        assert!(line.contains("2."));
        assert!(line.contains("preview"));
    }

    #[test]
    fn diff_header_mentions_window() {
        let header = diff_inspector_header(
            std::path::Path::new("/tmp/a.md"),
            std::path::Path::new("/tmp/b.md"),
            "different",
            4,
            40,
        );
        assert!(header.contains("Window: hunks=4 lines=40"));
    }

    #[test]
    fn timeline_anchor_panel_mentions_runtime_state() {
        let path = std::path::Path::new("/tmp/a.md");
        let meta = TranscriptMetadata {
            timestamp: Some("2026-01-01 00:00:00".to_string()),
            session_memory_path: Some("/tmp/memory.md".to_string()),
            ..TranscriptMetadata::default()
        };
        let panel = transcript_timeline_anchor_panel(path, &meta, None);
        assert!(panel.contains("Timeline anchors"));
        assert!(panel.contains("Transcript session memory"));
    }
}
