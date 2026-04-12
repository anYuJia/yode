use std::path::Path;

pub(crate) fn format_retry_delay_summary(delay_secs: u64, attempt: u32, max_attempts: u32) -> String {
    format!("Retrying in {}s ({}/{})", delay_secs, attempt, max_attempts)
}

pub(crate) fn format_context_compressed_message(
    mode: &str,
    removed: usize,
    tool_results_truncated: usize,
    summary: Option<&str>,
    session_memory_path: Option<&str>,
    transcript_path: Option<&str>,
) -> String {
    let mut content = match (removed, tool_results_truncated) {
        (0, truncated) => format!(
            "Context compressed ({}): truncated {} oversized tool results to stay within the window.",
            mode, truncated
        ),
        (removed, 0) => format!(
            "Context compressed ({}): removed {} messages to fit window.",
            mode, removed
        ),
        (removed, truncated) => format!(
            "Context compressed ({}): removed {} messages and truncated {} oversized tool results.",
            mode, removed, truncated
        ),
    };

    if let Some(summary) = summary {
        content.push('\n');
        content.push_str(summary);
    }
    if let Some(path) = session_memory_path {
        content.push_str("\nSession memory: ");
        content.push_str(path);
    }
    if let Some(path) = transcript_path {
        content.push_str("\nTranscript backup: ");
        content.push_str(path);
    }

    content
}

pub(crate) fn format_session_memory_update_message(path: &str, generated_summary: bool) -> String {
    format!(
        "Session memory updated ({}): {}",
        if generated_summary { "summary" } else { "snapshot" },
        path
    )
}

pub(crate) fn format_budget_exceeded_message(cost: f64, limit: f64) -> String {
    format!("⚠ Budget limit exceeded: ${:.4} (limit: ${:.2})", cost, limit)
}

pub(crate) fn format_tool_progress_summary(
    tool_name: Option<&str>,
    message: Option<&str>,
    at: Option<&str>,
) -> String {
    match (tool_name, message, at) {
        (None, None, None) => "none".to_string(),
        (Some(tool), Some(message), Some(at)) => format!("{}: {} @ {}", tool, message, at),
        (Some(tool), Some(message), None) => format!("{}: {}", tool, message),
        (Some(tool), None, Some(at)) => format!("{} @ {}", tool, at),
        (Some(tool), None, None) => tool.to_string(),
        (None, Some(message), Some(at)) => format!("{} @ {}", message, at),
        (None, Some(message), None) => message.to_string(),
        (None, None, Some(at)) => format!("updated @ {}", at),
    }
}

pub(crate) fn format_repeated_tool_failure_summary(summary: Option<&str>) -> String {
    let summary = summary.unwrap_or("none");
    if summary.chars().count() <= 120 {
        return summary.to_string();
    }
    format!("{}...", summary.chars().take(120).collect::<String>())
}

pub(crate) fn format_permission_decision_summary(
    tool: Option<&str>,
    action: Option<&str>,
    explanation: Option<&str>,
) -> String {
    format!(
        "{} [{}] {}",
        tool.unwrap_or("none"),
        action.unwrap_or("none"),
        explanation.unwrap_or("none")
    )
}

pub(crate) fn fold_recovery_breadcrumbs(breadcrumbs: &[String], max_items: usize) -> String {
    if breadcrumbs.is_empty() {
        return "none".to_string();
    }
    if breadcrumbs.len() <= max_items {
        return breadcrumbs.join(" -> ");
    }
    let tail = breadcrumbs[breadcrumbs.len() - max_items..].join(" -> ");
    format!("+{} earlier -> {}", breadcrumbs.len() - max_items, tail)
}

pub(crate) fn format_turn_artifact_status(path: Option<&str>) -> String {
    match path {
        None => "none".to_string(),
        Some(path) if Path::new(path).exists() => format!("present: {}", path),
        Some(path) => format!("missing: {}", path),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        fold_recovery_breadcrumbs, format_retry_delay_summary, format_tool_progress_summary,
        format_turn_artifact_status,
    };

    #[test]
    fn fold_recovery_breadcrumbs_compacts_older_entries() {
        let folded = fold_recovery_breadcrumbs(
            &[
                "parse".to_string(),
                "stream".to_string(),
                "tool".to_string(),
                "recover".to_string(),
            ],
            2,
        );
        assert_eq!(folded, "+2 earlier -> tool -> recover");
    }

    #[test]
    fn retry_delay_summary_formats_attempts() {
        assert_eq!(format_retry_delay_summary(5, 2, 5), "Retrying in 5s (2/5)");
    }

    #[test]
    fn tool_progress_summary_includes_timestamp_when_available() {
        assert_eq!(
            format_tool_progress_summary(Some("bash"), Some("running tests"), Some("10:00")),
            "bash: running tests @ 10:00"
        );
    }

    #[test]
    fn turn_artifact_status_reports_missing_paths() {
        assert_eq!(
            format_turn_artifact_status(Some("/definitely/missing/artifact.md")),
            "missing: /definitely/missing/artifact.md"
        );
    }
}
