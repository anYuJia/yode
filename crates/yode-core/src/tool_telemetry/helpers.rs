use serde_json::Value;
use sha2::{Digest, Sha256};

use yode_llm::types::ToolCall;

use crate::tool_runtime::{ToolResultTruncationView, ToolResultTruncationView as TruncationView};

use super::super::retry::hex_short;

pub(super) fn summarize_result_metadata(metadata: &Option<Value>) -> Option<String> {
    let meta = metadata.as_ref()?.as_object()?;
    let mut parts = Vec::new();
    for key in [
        "file_path",
        "byte_count",
        "line_count",
        "replacements",
        "applied_edits",
        "command_type",
        "rewrite_suggestion",
        "url",
        "count",
    ] {
        if let Some(value) = meta.get(key) {
            let rendered = if let Some(string) = value.as_str() {
                string.to_string()
            } else {
                value.to_string()
            };
            parts.push(format!("{}={}", key, rendered));
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

pub(super) fn extract_diff_preview(metadata: &Option<Value>) -> Option<String> {
    let diff = metadata
        .as_ref()
        .and_then(|meta| meta.get("diff_preview"))
        .and_then(|value| value.as_object())?;

    let mut lines = Vec::new();
    if let Some(removed) = diff.get("removed").and_then(|value| value.as_array()) {
        for line in removed.iter().filter_map(|value| value.as_str()) {
            lines.push(format!("-{}", line));
        }
        if let Some(extra) = diff.get("more_removed").and_then(|value| value.as_u64()) {
            if extra > 0 {
                lines.push(format!("... {} more removed", extra));
            }
        }
    }
    if let Some(added) = diff.get("added").and_then(|value| value.as_array()) {
        for line in added.iter().filter_map(|value| value.as_str()) {
            lines.push(format!("+{}", line));
        }
        if let Some(extra) = diff.get("more_added").and_then(|value| value.as_u64()) {
            if extra > 0 {
                lines.push(format!("... {} more added", extra));
            }
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

pub(super) fn output_preview(content: &str) -> String {
    const MAX_LINES: usize = 6;
    const MAX_CHARS: usize = 500;

    let lines = content.lines().take(MAX_LINES).collect::<Vec<_>>();
    let mut preview = lines.join("\n");
    if preview.chars().count() > MAX_CHARS {
        preview = preview.chars().take(MAX_CHARS).collect::<String>();
        preview.push_str("\n... [preview truncated]");
    } else if content.lines().count() > MAX_LINES {
        preview.push_str("\n... [more lines omitted]");
    }
    preview
}

pub(super) fn failure_signature(tool_call: &ToolCall, error_type: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(tool_call.name.as_bytes());
    hasher.update(tool_call.arguments.as_bytes());
    if let Some(kind) = error_type {
        hasher.update(kind.as_bytes());
    }
    let digest = hasher.finalize();
    format!(
        "{}:{}:{}",
        tool_call.name,
        error_type.unwrap_or("unknown"),
        hex_short(&digest)
    )
}

pub(super) fn tool_truncation_from_metadata(
    metadata: &Option<Value>,
) -> Option<ToolResultTruncationView> {
    let tool_runtime = metadata
        .as_ref()
        .and_then(|meta| meta.get("tool_runtime"))
        .and_then(|value| value.as_object())?;
    let truncation = tool_runtime.get("truncation")?.as_object()?;
    Some(TruncationView {
        reason: truncation
            .get("reason")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown")
            .to_string(),
        original_bytes: truncation
            .get("original_bytes")
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as usize,
        kept_bytes: truncation
            .get("kept_bytes")
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as usize,
        omitted_bytes: truncation
            .get("omitted_bytes")
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as usize,
    })
}
