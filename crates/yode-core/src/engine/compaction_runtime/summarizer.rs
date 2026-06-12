use regex::Regex;
use std::collections::HashSet;
use std::path::Path;
use yode_llm::types::{Message, Role};

pub(super) const SESSION_MEMORY_SUMMARY_PREFIX: &str = "[Context summary]";
pub(super) const SESSION_MEMORY_SUMMARY_MAX_CHARS: usize = 1_200;
pub(super) const LLM_COMPACTION_SUMMARY_MAX_CHARS: usize = 3_200;
pub(super) const LLM_COMPACTION_TRANSCRIPT_CHAR_BUDGET: usize = 28_000;
pub(super) const LLM_COMPACTION_MAX_RETRIES: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionMode {
    Auto,
    Manual,
    Reactive,
}

impl CompactionMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
            Self::Reactive => "reactive",
        }
    }

    pub fn is_auto(self) -> bool {
        matches!(self, Self::Auto)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionSummaryScope {
    Full,
    PartialUpTo,
    PartialFrom,
}

impl CompactionSummaryScope {
    pub fn prompt_guidance(self) -> &'static str {
        match self {
            Self::Full => {
                "Scope: full compact. Summarize the compacted conversation so the next turn can continue from the kept recent tail."
            }
            Self::PartialUpTo => {
                "Scope: partial compact up_to. You are summarizing the older prefix before a selected point. Preserve durable goals, decisions, files, constraints, and handoff state; do not imply the newer tail is included because it remains verbatim after this summary."
            }
            Self::PartialFrom => {
                "Scope: partial compact from. You are summarizing the later tail after a selected point. Earlier messages remain verbatim before this summary, so focus on actionable work, findings, and next steps from the summarized tail."
            }
        }
    }
}

pub(super) fn is_prompt_too_long_text(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    normalized.contains("prompt too long")
        || normalized.contains("context window")
        || normalized.contains("context length")
        || normalized.contains("maximum context")
        || normalized.contains("too many tokens")
        || normalized.contains("input is too long")
        || normalized.contains("input tokens")
}

pub(super) fn is_media_size_error_text(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    (normalized.contains("image exceeds") && normalized.contains("maximum"))
        || normalized.contains("image dimensions exceed")
        || normalized.contains("too many images")
        || normalized.contains("maximum of") && normalized.contains("pdf pages")
        || normalized.contains("image was too large")
        || normalized.contains("request too large")
        || normalized.contains("file was too large")
}

pub(super) fn parse_prompt_too_long_token_gap(text: &str) -> Option<usize> {
    let regex =
        Regex::new(r"prompt(?:\s+is)?\s+too\s+long[^0-9]*(\d+)\s*tokens?\s*>\s*(\d+)").ok()?;
    let captures = regex.captures(text)?;
    let actual = captures.get(1)?.as_str().parse::<usize>().ok()?;
    let limit = captures.get(2)?.as_str().parse::<usize>().ok()?;
    actual.checked_sub(limit).filter(|gap| *gap > 0)
}

pub(super) fn display_compaction_memory_path(project_root: &Path, path: &Path) -> String {
    path.strip_prefix(project_root)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

pub(super) fn build_session_memory_compaction_summary(
    project_root: &Path,
    path: &Path,
    excerpt: &str,
) -> String {
    let mut summary = format!(
        "{} Earlier conversation was compacted using persisted session memory.\n- Session memory source: {}\n- Carry-over:\n{}",
        SESSION_MEMORY_SUMMARY_PREFIX,
        display_compaction_memory_path(project_root, path),
        excerpt
    );
    if summary.chars().count() > SESSION_MEMORY_SUMMARY_MAX_CHARS {
        summary = summary
            .chars()
            .take(SESSION_MEMORY_SUMMARY_MAX_CHARS)
            .collect::<String>();
        summary.push_str("...");
    }
    summary
}

pub(super) fn summarize_string_entries(entries: &[String], max_items: usize) -> Option<String> {
    if entries.is_empty() {
        return None;
    }

    let mut values = entries.to_vec();
    values.sort();
    values.dedup();
    let extra = values.len().saturating_sub(max_items);
    values.truncate(max_items);
    let mut summary = values.join(", ");
    if extra > 0 {
        summary.push_str(&format!(", +{} more", extra));
    }
    Some(summary)
}

pub(super) fn compact_summary_fingerprint(summary: Option<&String>) -> Option<String> {
    let summary = summary?.trim();
    if summary.is_empty() {
        return None;
    }

    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(summary.as_bytes());
    Some(format!("{:x}", hasher.finalize())[..16].to_string())
}

pub(super) fn message_boundary_key(message: &Message) -> String {
    serde_json::json!({
        "role": format!("{:?}", message.role),
        "content": message.content,
        "reasoning": message.reasoning,
        "tool_calls": message.tool_calls,
        "tool_call_id": message.tool_call_id,
    })
    .to_string()
}

pub(super) fn preserved_tail_range(
    pre_compact_messages: &[Message],
    post_compact_messages: &[Message],
) -> Option<String> {
    let mut pre_index = pre_compact_messages.len();
    let mut post_index = post_compact_messages.len();

    while pre_index > 0 && post_index > 0 {
        let pre_key = message_boundary_key(&pre_compact_messages[pre_index - 1]);
        let post_key = message_boundary_key(&post_compact_messages[post_index - 1]);
        if pre_key != post_key {
            break;
        }
        pre_index -= 1;
        post_index -= 1;
    }

    (pre_index < pre_compact_messages.len())
        .then(|| format!("{}..{}", pre_index, pre_compact_messages.len()))
}

pub(super) fn push_artifact_path(artifact_paths: &mut Vec<String>, path: Option<&Path>) {
    let Some(path) = path else {
        return;
    };
    let path = path.display().to_string();
    if !artifact_paths.contains(&path) {
        artifact_paths.push(path);
    }
}

pub(super) fn message_excerpt_for_compaction(message: &Message, limit: usize) -> Option<String> {
    let role = match message.role {
        Role::System => "System",
        Role::User => "User",
        Role::Assistant => "Assistant",
        Role::Tool => "Tool",
    };

    let mut body = String::new();
    if let Some(content) = message.content.as_deref() {
        let squashed = content.split_whitespace().collect::<Vec<_>>().join(" ");
        if !squashed.is_empty() {
            body.push_str(&squashed.chars().take(limit).collect::<String>());
            if squashed.chars().count() > limit {
                body.push_str("...");
            }
        }
    }

    if !message.tool_calls.is_empty() {
        if !body.is_empty() {
            body.push_str(" | ");
        }
        let names = message
            .tool_calls
            .iter()
            .map(|call| call.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        body.push_str(&format!("tool_calls: {}", names));
    }

    if let Some(tool_call_id) = message.tool_call_id.as_deref() {
        if !body.is_empty() {
            body.push_str(" | ");
        }
        body.push_str(&format!("tool_result_for: {}", tool_call_id));
    }

    (!body.trim().is_empty()).then(|| format!("{}: {}", role, body.trim()))
}

pub(super) fn render_removed_messages_for_summary(
    messages: &[Message],
    char_budget: usize,
) -> String {
    let mut rendered = String::new();
    for line in messages
        .iter()
        .filter_map(|message| message_excerpt_for_compaction(message, 420))
    {
        let line_with_break = format!("- {}\n", line);
        if rendered.chars().count() + line_with_break.chars().count() > char_budget {
            break;
        }
        rendered.push_str(&line_with_break);
    }
    rendered
}

pub(super) fn truncate_head_for_summary_retry(
    messages: &[Message],
    error_text: &str,
) -> Vec<Message> {
    if messages.len() <= 2 {
        return Vec::new();
    }

    if let Some(token_gap) = parse_prompt_too_long_token_gap(error_text) {
        let mut accumulated = 0usize;
        let mut drop_count = 0usize;
        for message in messages {
            accumulated = accumulated.saturating_add(message.estimated_char_count() / 4);
            drop_count = drop_count.saturating_add(1);
            if accumulated >= token_gap {
                break;
            }
        }
        if drop_count > 0 && drop_count < messages.len() {
            return messages.iter().skip(drop_count).cloned().collect();
        }
    }

    let drop_count = (messages.len() / 5).max(1);
    messages.iter().skip(drop_count).cloned().collect()
}

pub(super) fn collect_assistant_tool_call_ids(messages: &[Message]) -> HashSet<String> {
    messages
        .iter()
        .filter(|message| matches!(message.role, Role::Assistant))
        .flat_map(|message| message.tool_calls.iter().map(|call| call.id.clone()))
        .collect()
}

pub(super) fn collect_tool_result_ids(messages: &[Message]) -> HashSet<String> {
    messages
        .iter()
        .filter(|message| matches!(message.role, Role::Tool))
        .filter_map(|message| message.tool_call_id.clone())
        .collect()
}

pub(super) fn build_fallback_compaction_summary(
    removed_messages: &[Message],
    turn_artifact_path: Option<&str>,
) -> String {
    let mut lines = vec![
        format!(
            "{} Older conversation was compacted to preserve working context.",
            SESSION_MEMORY_SUMMARY_PREFIX
        ),
        format!("- Removed messages: {}", removed_messages.len()),
    ];

    if let Some(path) = turn_artifact_path.filter(|path| !path.trim().is_empty()) {
        lines.push(format!("- Turn artifact: {}", path));
    }

    let highlights = removed_messages
        .iter()
        .filter_map(|message| message_excerpt_for_compaction(message, 140))
        .take(4)
        .collect::<Vec<_>>();
    if !highlights.is_empty() {
        lines.push("- Highlights:".to_string());
        for highlight in highlights {
            lines.push(format!("  - {}", highlight));
        }
    }

    let mut summary = lines.join("\n");
    if summary.chars().count() > SESSION_MEMORY_SUMMARY_MAX_CHARS {
        summary = summary
            .chars()
            .take(SESSION_MEMORY_SUMMARY_MAX_CHARS)
            .collect::<String>();
        summary.push_str("...");
    }
    summary
}

pub(super) fn format_llm_compaction_summary_content(raw: &str) -> Option<String> {
    let mut content = raw.trim().to_string();
    if content.is_empty() {
        return None;
    }

    let analysis_re = Regex::new(r"(?is)<analysis>.*?</analysis>").ok()?;
    content = analysis_re.replace_all(&content, "").to_string();

    let summary_re = Regex::new(r"(?is)<summary>(.*?)</summary>").ok()?;
    if let Some(captures) = summary_re.captures(&content) {
        content = captures
            .get(1)
            .map(|matched| matched.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
    }

    let blank_re = Regex::new(r"\n{3,}").ok()?;
    content = blank_re.replace_all(content.trim(), "\n\n").to_string();

    (!content.trim().is_empty()).then(|| content.trim().to_string())
}

pub(super) fn prompt_cache_value(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}

pub(super) fn prompt_cache_text_value(value: Option<&String>) -> &str {
    value.map(|value| value.as_str()).unwrap_or("none")
}
