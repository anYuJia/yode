use super::*;
use super::summary::build_context_summary;

use tracing::info;

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

impl ContextManager {
    fn build_summary(
        &self,
        removed_messages: &[Message],
        tool_results_truncated: usize,
    ) -> Option<String> {
        build_context_summary(removed_messages, tool_results_truncated)
    }

    pub fn compress_with_report(&self, messages: &mut Vec<Message>) -> CompressionReport {
        let original_len = messages.len();
        let mut report = CompressionReport::default();

        if messages.len() <= PRESERVE_RECENT + 1 {
            return report;
        }

        let preserve_start = 1;
        let preserve_end = messages.len().saturating_sub(PRESERVE_RECENT);
        let mut removed_messages = Vec::new();

        for msg in messages[preserve_start..preserve_end].iter_mut() {
            if matches!(msg.role, Role::Tool) {
                if let Some(content) = msg.content.as_ref() {
                    if content.len() > COMPRESSED_TOOL_RESULT_MAX {
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
        let target_tokens = (self.limits.context_window as f64 * 0.6) as usize;

        if estimated_tokens <= target_tokens {
            if report.tool_results_truncated > 0 {
                info!(
                    "Context compressed: truncated {} oversized tool results",
                    report.tool_results_truncated
                );
            }
            return report;
        }

        while messages.len() > PRESERVE_RECENT + 1 {
            let current_estimate = self.estimate_tokens(messages);
            if current_estimate <= target_tokens {
                break;
            }

            let remove_end = messages.len().saturating_sub(PRESERVE_RECENT);
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

            let removed_msg = messages.remove(min_index);
            let role = removed_msg.role.clone();
            removed_messages.push(removed_msg);
            report.removed += 1;

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
                        report.removed += 1;
                    }
                }
            }
        }

        if report.removed > 0 {
            if let Some(summary) =
                self.build_summary(&removed_messages, report.tool_results_truncated)
            {
                let insert_at = messages.len().saturating_sub(PRESERVE_RECENT).max(1);
                messages.insert(insert_at, Message::system(summary.clone()));
                report.summary = Some(summary);
            }
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

    pub fn compress(&self, messages: &mut Vec<Message>) -> usize {
        self.compress_with_report(messages).removed
    }
}
