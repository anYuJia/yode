use super::summary::{build_context_summary, excerpt};
use super::*;

use tracing::info;

const COMPACTION_TARGET_THRESHOLD: f64 = 0.78;

pub(crate) fn is_context_summary(msg: &Message) -> bool {
    matches!(msg.role, Role::System)
        && msg
            .content
            .as_deref()
            .unwrap_or_default()
            .starts_with(CONTEXT_SUMMARY_PREFIX)
}

pub(crate) fn message_priority(msg: &Message) -> u32 {
    if is_context_summary(msg) {
        return 2;
    }

    match msg.role {
        Role::System => 99,
        Role::Assistant if !msg.tool_calls.is_empty() => 3,
        Role::Tool => 2,
        _ => 1,
    }
}

fn is_microcompacted_tool_result(msg: &Message) -> bool {
    matches!(msg.role, Role::Tool)
        && msg
            .content
            .as_deref()
            .unwrap_or_default()
            .starts_with(MICROCOMPACT_PREFIX)
}

fn summarize_tool_result_for_microcompact(content: &str) -> String {
    let preview = excerpt(content, MICROCOMPACT_PREVIEW_CHARS).unwrap_or_else(|| "n/a".to_string());
    format!(
        "{} Older tool output was cleared to protect context capacity. Original length: {} chars. Preview: {}",
        MICROCOMPACT_PREFIX,
        content.chars().count(),
        preview
    )
}

fn image_payload_chars(msg: &Message) -> usize {
    msg.images
        .iter()
        .map(|image| image.base64.len().saturating_add(image.media_type.len()))
        .sum()
}

fn append_microcompact_media_marker(msg: &mut Message, removed: usize, saved_chars: usize) {
    let marker = format!(
        "[Older media microcompacted: removed {} attachment(s), saved ~{} chars]",
        removed, saved_chars
    );
    match msg.content.as_mut() {
        Some(content) if !content.contains("[Older media microcompacted:") => {
            if !content.ends_with('\n') {
                content.push_str("\n\n");
            }
            content.push_str(&marker);
        }
        None => {
            msg.content = Some(marker);
        }
        _ => {}
    }
}

impl ContextManager {
    fn build_summary(
        &self,
        removed_messages: &[Message],
        tool_results_truncated: usize,
        turn_artifact_path: Option<&str>,
    ) -> Option<String> {
        build_context_summary(removed_messages, tool_results_truncated, turn_artifact_path)
    }

    pub fn compress_with_report(&self, messages: &mut Vec<Message>) -> CompressionReport {
        self.compress_with_turn_artifact(messages, None)
    }

    pub fn compress_with_keep_last(
        &self,
        messages: &mut Vec<Message>,
        keep_last: usize,
        turn_artifact_path: Option<&str>,
    ) -> CompressionReport {
        let keep_last = keep_last.max(1);
        self.force_compact_middle(messages, keep_last, turn_artifact_path)
    }

    pub fn compact_with_external_summary(
        &self,
        messages: &mut Vec<Message>,
        keep_last: usize,
        summary: String,
    ) -> CompressionReport {
        let keep_last = keep_last.max(1);
        let mut report = CompressionReport::default();

        if summary.trim().is_empty() || messages.len() <= keep_last + 1 {
            return report;
        }

        let remove_end = messages.len().saturating_sub(keep_last);
        if remove_end <= 1 {
            return report;
        }

        report.removed = remove_end.saturating_sub(1);
        report.removed_messages = messages.drain(1..remove_end).collect();
        messages.insert(1, Message::system(summary.clone()));
        report.summary = Some(summary);
        report
    }

    pub fn microcompact(&self, messages: &mut [Message]) -> MicrocompactReport {
        let mut report = MicrocompactReport::default();
        if messages.len() <= MICROCOMPACT_PRESERVE_RECENT + 1 {
            return report;
        }

        let preserve_end = messages.len().saturating_sub(MICROCOMPACT_PRESERVE_RECENT);
        for msg in messages.iter_mut().take(preserve_end).skip(1) {
            if !matches!(msg.role, Role::Tool) || is_microcompacted_tool_result(msg) {
                continue;
            }

            let Some(content) = msg.content.as_ref() else {
                continue;
            };
            let original_chars = content.chars().count();
            if original_chars < MICROCOMPACT_TRIGGER_CHARS {
                continue;
            }

            let compacted = summarize_tool_result_for_microcompact(content);
            if compacted == *content {
                continue;
            }

            report.tool_results_cleared = report.tool_results_cleared.saturating_add(1);
            report.saved_chars = report
                .saved_chars
                .saturating_add(original_chars.saturating_sub(compacted.chars().count()));
            msg.content = Some(compacted);
            msg.normalize_in_place();
        }

        report
    }

    pub fn microcompact_old_media(&self, messages: &mut [Message]) -> MicrocompactReport {
        let mut report = MicrocompactReport::default();
        if messages.len() <= MICROCOMPACT_PRESERVE_RECENT + 1 {
            return report;
        }

        let preserve_end = messages.len().saturating_sub(MICROCOMPACT_PRESERVE_RECENT);
        let older_media_payload_chars = messages
            .iter()
            .take(preserve_end)
            .skip(1)
            .map(image_payload_chars)
            .sum::<usize>();
        let has_collective_pressure =
            older_media_payload_chars >= MICROCOMPACT_MEDIA_TOTAL_TRIGGER_CHARS;

        for msg in messages.iter_mut().take(preserve_end).skip(1) {
            if msg.images.is_empty() {
                continue;
            }

            let saved_chars = image_payload_chars(msg);
            if saved_chars < MICROCOMPACT_MEDIA_TRIGGER_CHARS && !has_collective_pressure {
                continue;
            }

            let removed = msg.images.len();
            msg.images.clear();
            append_microcompact_media_marker(msg, removed, saved_chars);
            msg.normalize_in_place();
            report.media_removed = report.media_removed.saturating_add(removed);
            report.saved_chars = report.saved_chars.saturating_add(saved_chars);
        }

        report
    }

    pub fn collect_microcompact_cache_refs(&self, messages: &[Message]) -> Vec<String> {
        if messages.len() <= MICROCOMPACT_PRESERVE_RECENT + 1 {
            return Vec::new();
        }

        let preserve_end = messages.len().saturating_sub(MICROCOMPACT_PRESERVE_RECENT);
        messages
            .iter()
            .take(preserve_end)
            .skip(1)
            .filter(|msg| matches!(msg.role, Role::Tool))
            .filter_map(|msg| {
                let content = msg.content.as_ref()?;
                (content.chars().count() >= MICROCOMPACT_TRIGGER_CHARS)
                    .then(|| msg.tool_call_id.clone())
                    .flatten()
            })
            .collect()
    }

    pub fn compress_with_turn_artifact(
        &self,
        messages: &mut Vec<Message>,
        turn_artifact_path: Option<&str>,
    ) -> CompressionReport {
        self.compress_with_preserve_recent(messages, PRESERVE_RECENT, turn_artifact_path)
    }

    pub fn compress(&self, messages: &mut Vec<Message>) -> usize {
        self.compress_with_report(messages).removed
    }

    fn compress_with_preserve_recent(
        &self,
        messages: &mut Vec<Message>,
        preserve_recent: usize,
        turn_artifact_path: Option<&str>,
    ) -> CompressionReport {
        let original_len = messages.len();
        let mut report = CompressionReport::default();

        if messages.len() <= preserve_recent + 1 {
            return report;
        }

        let preserve_end = messages.len().saturating_sub(preserve_recent);
        for msg in messages.iter_mut().take(preserve_end).skip(1) {
            if matches!(msg.role, Role::Tool) {
                if let Some(content) = msg.content.as_ref() {
                    if content.chars().count() > COMPRESSED_TOOL_RESULT_MAX
                        && !is_microcompacted_tool_result(msg)
                    {
                        let truncated: String =
                            content.chars().take(COMPRESSED_TOOL_RESULT_MAX).collect();
                        msg.content = Some(format!("{}... [compressed]", truncated));
                        msg.normalize_in_place();
                        report.tool_results_truncated += 1;
                    }
                }
            }
        }

        let estimated_tokens = self.estimate_tokens(messages);
        let target_tokens =
            (self.limits.context_window as f64 * COMPACTION_TARGET_THRESHOLD) as usize;

        if estimated_tokens <= target_tokens {
            if report.tool_results_truncated > 0 {
                info!(
                    "Context compressed: truncated {} oversized tool results",
                    report.tool_results_truncated
                );
            }
            return report;
        }

        let mut removed_messages = Vec::new();
        while messages.len() > preserve_recent + 1 {
            let current_estimate = self.estimate_tokens(messages);
            if current_estimate <= target_tokens {
                break;
            }

            let remove_end = messages.len().saturating_sub(preserve_recent);
            if remove_end <= 1 {
                break;
            }

            let mut min_priority = u32::MAX;
            let mut min_index = 1;
            for (index, msg) in messages.iter().enumerate().take(remove_end).skip(1) {
                let priority = message_priority(msg);
                if priority < min_priority {
                    min_priority = priority;
                    min_index = index;
                }
            }

            Self::remove_message_with_linked_assistant(
                messages,
                min_index,
                &mut removed_messages,
                &mut report,
            );
        }

        if report.removed > 0 {
            if let Some(summary) = self.build_summary(
                &removed_messages,
                report.tool_results_truncated,
                turn_artifact_path,
            ) {
                let insert_at = messages.len().saturating_sub(preserve_recent).max(1);
                messages.insert(insert_at, Message::system(summary.clone()));
                report.summary = Some(summary);
            }
            report.removed_messages = removed_messages;
        }

        if report.removed > 0 || report.tool_results_truncated > 0 {
            info!(
                "Context compressed: removed {} messages, truncated {} tool results{}",
                report.removed,
                report.tool_results_truncated,
                if report.summary.is_some() {
                    ", inserted summary anchor"
                } else {
                    ""
                }
            );
        }
        debug_assert!(
            report.removed == original_len.saturating_sub(messages.len())
                || report.summary.is_some(),
            "compression report should match removed count unless a summary anchor was inserted"
        );
        report
    }

    fn force_compact_middle(
        &self,
        messages: &mut Vec<Message>,
        keep_last: usize,
        turn_artifact_path: Option<&str>,
    ) -> CompressionReport {
        let mut report = CompressionReport::default();
        if messages.len() <= keep_last + 1 {
            return report;
        }

        let remove_end = messages.len().saturating_sub(keep_last);
        if remove_end <= 1 {
            return report;
        }

        let removed_messages = messages.drain(1..remove_end).collect::<Vec<_>>();
        report.removed = removed_messages.len();
        if let Some(summary) = self.build_summary(&removed_messages, 0, turn_artifact_path) {
            messages.insert(1, Message::system(summary.clone()));
            report.summary = Some(summary);
        }
        report.removed_messages = removed_messages;
        report
    }

    fn remove_message_with_linked_assistant(
        messages: &mut Vec<Message>,
        min_index: usize,
        removed_messages: &mut Vec<Message>,
        report: &mut CompressionReport,
    ) {
        let removed_msg = messages.remove(min_index);
        let role = removed_msg.role.clone();
        removed_messages.push(removed_msg);
        report.removed = report.removed.saturating_add(1);

        if matches!(role, Role::Tool) && min_index > 0 {
            let prev = min_index - 1;
            if prev < messages.len()
                && matches!(messages[prev].role, Role::Assistant)
                && !messages[prev].tool_calls.is_empty()
            {
                let tool_call_ids: Vec<String> = messages[prev]
                    .tool_calls
                    .iter()
                    .map(|tool_call| tool_call.id.clone())
                    .collect();
                let has_results = messages.iter().any(|message| {
                    matches!(message.role, Role::Tool)
                        && message
                            .tool_call_id
                            .as_ref()
                            .map(|id| tool_call_ids.contains(id))
                            .unwrap_or(false)
                });
                if !has_results {
                    removed_messages.push(messages.remove(prev));
                    report.removed = report.removed.saturating_add(1);
                }
            }
        }
    }
}
