use super::*;

pub(in crate::commands::info::memory) fn read_transcript_metadata(
    path: &Path,
) -> Option<TranscriptMetadata> {
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

pub(in crate::commands::info::memory) fn extract_summary_preview(
    content: &str,
) -> Option<String> {
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

pub(in crate::commands::info::memory) fn transcript_picker_summary_preview(
    path: &Path,
) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let preview = extract_summary_preview(&content)?;
    if preview.chars().count() <= 100 {
        Some(preview)
    } else {
        Some(format!("{}...", preview.chars().take(100).collect::<String>()))
    }
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

pub(crate) fn run_long_session_benchmark(project_root: &Path) -> LongSessionBenchmarkReport {
    let transcripts_dir = project_root.join(".yode").join("transcripts");
    let entries = sorted_transcript_entries(&transcripts_dir);
    let transcript_count = entries.len();

    clear_transcript_caches();
    let cold_latest_lookup_ms = measure_ms(|| {
        let _ = latest_transcript(&transcripts_dir);
    });
    let hot_latest_lookup_ms = measure_ms(|| {
        let _ = latest_transcript(&transcripts_dir);
    });

    clear_transcript_caches();
    let failed_filter = TranscriptListFilter {
        require_failed: true,
        ..TranscriptListFilter::default()
    };
    let cold_failed_filter_ms = measure_ms(|| {
        let _ = filtered_transcript_entries(&transcripts_dir, &failed_filter);
    });
    let hot_failed_filter_ms = measure_ms(|| {
        let _ = filtered_transcript_entries(&transcripts_dir, &failed_filter);
    });

    let resume_warmup = warm_resume_transcript_caches(project_root);

    let compare_pair = if entries.len() >= 2 {
        Some((
            entries[0].display().to_string(),
            entries[1].display().to_string(),
        ))
    } else {
        None
    };
    let (compare_ms, compare_summary_only) = if entries.len() >= 2 {
        let left_path = &entries[0];
        let right_path = &entries[1];
        match (fs::read_to_string(left_path), fs::read_to_string(right_path)) {
            (Ok(left_content), Ok(right_content)) => {
                let mut summary_only = false;
                let elapsed = measure_ms(|| {
                    let output = build_transcript_compare_output(
                        left_path,
                        &left_content,
                        right_path,
                        &right_content,
                        &CompareOptions::default(),
                    );
                    summary_only = output.contains("skipped: content too large");
                });
                (Some(elapsed), Some(summary_only))
            }
            _ => (None, None),
        }
    } else {
        (None, None)
    };

    LongSessionBenchmarkReport {
        transcript_dir: transcripts_dir,
        transcript_count,
        cold_latest_lookup_ms,
        hot_latest_lookup_ms,
        cold_failed_filter_ms,
        hot_failed_filter_ms,
        resume_warmup,
        compare_pair,
        compare_ms,
        compare_summary_only,
    }
}

fn clear_transcript_caches() {
    if let Ok(mut cache) = TRANSCRIPT_META_CACHE.lock() {
        cache.clear();
    }
    if let Ok(mut cache) = LATEST_TRANSCRIPT_CACHE.lock() {
        cache.clear();
    }
}

fn measure_ms(f: impl FnOnce()) -> u64 {
    let started_at = Instant::now();
    f();
    started_at.elapsed().as_millis() as u64
}
