use anyhow::{anyhow, Result};

use super::types::{RemoteQueueItem, RemoteTransportPayload};

pub(super) fn truncate_preview(text: &str, max_chars: usize) -> String {
    let squashed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if squashed.chars().count() <= max_chars {
        squashed
    } else {
        format!(
            "{}...",
            squashed.chars().take(max_chars).collect::<String>()
        )
    }
}

pub(super) fn sanitize_label(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

pub(super) fn normalize_result_status(raw: &str) -> Result<&'static str> {
    match raw.trim() {
        "completed" | "complete" | "success" => Ok("completed"),
        "failed" | "fail" | "error" => Ok("failed"),
        "ack" | "acked" | "acknowledged" => Ok("acked"),
        other => Err(anyhow!(
            "Unsupported result status '{}'. Expected completed|failed|acknowledged.",
            other
        )),
    }
}

pub(super) fn queue_status_label(status: &str) -> &str {
    match status {
        "planned" | "queued" => "queued",
        "dispatched" => "dispatched",
        "running" => "running",
        "completed" => "completed",
        "failed" => "failed",
        "acked" => "acknowledged",
        "attention" => "needs-attention",
        other => other,
    }
}

pub(super) fn summarize_queue_status(items: &[RemoteQueueItem]) -> String {
    if items
        .iter()
        .any(|item| matches!(item.status.as_str(), "running" | "dispatched"))
    {
        "running".to_string()
    } else if items.iter().all(|item| item.status == "acked") {
        "acked".to_string()
    } else if items.iter().any(|item| item.status == "failed") {
        "attention".to_string()
    } else if items.iter().all(|item| item.status == "completed") {
        "completed".to_string()
    } else {
        "queued".to_string()
    }
}

pub(super) fn transport_block_reason(payload: &RemoteTransportPayload) -> String {
    match payload.connection_status.as_str() {
        "error" => format!(
            "transport error ({})",
            payload.last_error.as_deref().unwrap_or("unknown")
        ),
        "reconnecting" => "transport reconnecting".to_string(),
        "connected" => "transport connected".to_string(),
        status => format!("transport {}", status),
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_result_status, queue_status_label, sanitize_label, truncate_preview};

    #[test]
    fn queue_labels_match_operator_surface() {
        assert_eq!(queue_status_label("planned"), "queued");
        assert_eq!(queue_status_label("acked"), "acknowledged");
        assert_eq!(queue_status_label("attention"), "needs-attention");
    }

    #[test]
    fn normalizes_result_status_aliases() {
        assert_eq!(normalize_result_status("success").unwrap(), "completed");
        assert_eq!(normalize_result_status("acknowledged").unwrap(), "acked");
        assert!(normalize_result_status("paused").is_err());
    }

    #[test]
    fn preview_and_label_helpers_are_stable() {
        assert_eq!(sanitize_label("My Host.local"), "my-host-local");
        assert_eq!(truncate_preview("a   b\nc", 20), "a b c");
        assert_eq!(truncate_preview("abcdef", 3), "abc...");
    }
}
