use crate::app_bootstrap::tooling::ToolingSetupMetrics;

pub(crate) fn append_startup_segment(summary: &mut String, segment: &str) {
    if !segment.trim().is_empty() {
        summary.push(' ');
        summary.push_str(segment);
    }
}

pub(crate) fn append_startup_segments<'a, I>(summary: &mut String, segments: I)
where
    I: IntoIterator<Item = &'a str>,
{
    for segment in segments {
        append_startup_segment(summary, segment);
    }
}

pub(crate) fn build_startup_resume_segment(
    db_open_elapsed_ms: u64,
    session_bootstrap_elapsed_ms: u64,
    restored_messages: usize,
    restore_mode: &str,
    decoded_messages: usize,
    skipped_messages: usize,
    fallback_reason: Option<&str>,
) -> String {
    format!(
        "resume[db_open={}ms session_bootstrap={}ms restored_messages={} restore_mode={} decoded={} skipped={} fallback={}]",
        db_open_elapsed_ms,
        session_bootstrap_elapsed_ms,
        restored_messages,
        restore_mode,
        decoded_messages,
        skipped_messages,
        fallback_reason.unwrap_or("none")
    )
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn build_resume_warmup_segment(
    transcript_count: usize,
    metadata_entries_warmed: usize,
    latest_lookup_cached: bool,
    duration_ms: u64,
) -> String {
    format!(
        "resume_warmup[transcripts={} metadata={} latest={} duration={}ms]",
        transcript_count,
        metadata_entries_warmed,
        if latest_lookup_cached { "yes" } else { "no" },
        duration_ms,
    )
}

pub(crate) fn parse_startup_summary_segment(summary: &str, key: &str) -> Option<String> {
    let needle = format!("{}[", key);
    let start = summary.find(&needle)?;
    let rest = &summary[start..];
    let end = rest.find(']')?;
    Some(rest[..=end].to_string())
}

pub(crate) fn tooling_phase_summary(tooling: &ToolingSetupMetrics) -> String {
    format!(
        "tooling[builtin={}ms mcp_connect={}ms mcp_register={}ms skills={}ms total={}ms]",
        tooling.builtin_register_ms,
        tooling.mcp_connect_ms,
        tooling.mcp_register_ms,
        tooling.skill_discovery_ms,
        tooling.total_ms
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_summary_helpers_append_and_parse_segments() {
        let mut summary = "base".to_string();
        append_startup_segments(&mut summary, ["resume[db_open=1ms]", ""]);
        assert_eq!(summary, "base resume[db_open=1ms]");
        assert_eq!(
            parse_startup_summary_segment(&summary, "resume").as_deref(),
            Some("resume[db_open=1ms]")
        );
    }

    #[test]
    fn builds_resume_warmup_segment() {
        assert_eq!(
            build_resume_warmup_segment(3, 7, true, 42),
            "resume_warmup[transcripts=3 metadata=7 latest=yes duration=42ms]"
        );
    }
}
