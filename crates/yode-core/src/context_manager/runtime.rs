use super::*;

use tracing::info;

impl ModelLimits {
    /// Look up known model limits by model name.
    pub fn for_model(model: &str) -> Self {
        let model_lower = model.to_lowercase();
        if model_lower.contains("claude-sonnet-4") || model_lower.contains("claude-3-5-sonnet") {
            Self {
                context_window: 200_000,
                output_tokens: 8_192,
            }
        } else if model_lower.contains("claude-opus")
            || model_lower.contains("claude-3-opus")
            || model_lower.contains("claude-haiku")
            || model_lower.contains("claude-3-haiku")
        {
            Self {
                context_window: 200_000,
                output_tokens: 4_096,
            }
        } else if model_lower.contains("gpt-4o") || model_lower.contains("gpt-4-turbo") {
            Self {
                context_window: 128_000,
                output_tokens: 4_096,
            }
        } else if model_lower.contains("gpt-3.5") {
            Self {
                context_window: 16_385,
                output_tokens: 4_096,
            }
        } else {
            Self {
                context_window: 128_000,
                output_tokens: 4_096,
            }
        }
    }
}

pub(in crate::context_manager) fn is_context_summary(msg: &Message) -> bool {
    matches!(msg.role, Role::System)
        && msg
            .content
            .as_deref()
            .unwrap_or_default()
            .starts_with(CONTEXT_SUMMARY_PREFIX)
}

/// Message removal priority (lower = removed first).
pub(in crate::context_manager) fn message_priority(msg: &Message) -> u32 {
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
    pub fn new(model: &str) -> Self {
        Self {
            limits: ModelLimits::for_model(model),
            threshold: 0.75,
            last_known_prompt_tokens: None,
            last_known_char_count: None,
        }
    }

    /// Check if the current token usage suggests we should compress.
    pub fn should_compress(&mut self, prompt_tokens: u32, messages: &[Message]) -> bool {
        self.last_known_prompt_tokens = Some(prompt_tokens);
        let char_count: usize = messages
            .iter()
            .map(|m| {
                let content_len = m.content.as_ref().map(|c| c.len()).unwrap_or(0);
                let tool_calls_len: usize = m
                    .tool_calls
                    .iter()
                    .map(|tc| tc.arguments.len() + tc.name.len())
                    .sum();
                content_len + tool_calls_len
            })
            .sum();
        self.last_known_char_count = Some(char_count);
        (prompt_tokens as f64) > (self.limits.context_window as f64 * self.threshold)
    }

    /// Estimate token count for the given messages.
    pub(in crate::context_manager) fn estimate_tokens(&self, messages: &[Message]) -> usize {
        let char_count: usize = messages
            .iter()
            .map(|m| {
                let content_len = m.content.as_ref().map(|c| c.len()).unwrap_or(0);
                let tool_calls_len: usize = m
                    .tool_calls
                    .iter()
                    .map(|tc| tc.arguments.len() + tc.name.len())
                    .sum();
                content_len + tool_calls_len
            })
            .sum();

        if let Some(known_tokens) = self.last_known_prompt_tokens {
            if let Some(known_chars) = self.last_known_char_count {
                if known_chars > 0 {
                    return ((char_count as f64) * (known_tokens as f64 / known_chars as f64))
                        as usize;
                }
            }
        }

        char_count / 4
    }

    fn excerpt(text: &str, limit: usize) -> Option<String> {
        let squashed = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if squashed.is_empty() {
            return None;
        }

        let shortened: String = squashed.chars().take(limit).collect();
        if squashed.chars().count() > limit {
            Some(format!("{}...", shortened.trim_end()))
        } else {
            Some(shortened)
        }
    }

    fn build_summary(
        &self,
        removed_messages: &[Message],
        tool_results_truncated: usize,
    ) -> Option<String> {
        let mut user_goals = Vec::new();
        let mut assistant_findings = Vec::new();
        let mut tool_usage: BTreeMap<String, usize> = BTreeMap::new();
        let mut removed_tool_results = 0usize;

        for msg in removed_messages {
            if is_context_summary(msg) {
                continue;
            }

            match msg.role {
                Role::User => {
                    if user_goals.len() < 3 {
                        if let Some(content) = msg.content.as_deref() {
                            if let Some(excerpt) = Self::excerpt(content, 120) {
                                if !user_goals.contains(&excerpt) {
                                    user_goals.push(excerpt);
                                }
                            }
                        }
                    }
                }
                Role::Assistant => {
                    for tc in &msg.tool_calls {
                        *tool_usage.entry(tc.name.clone()).or_insert(0) += 1;
                    }
                    if msg.tool_calls.is_empty() && assistant_findings.len() < 3 {
                        if let Some(content) = msg.content.as_deref() {
                            if let Some(excerpt) = Self::excerpt(content, 140) {
                                if !assistant_findings.contains(&excerpt) {
                                    assistant_findings.push(excerpt);
                                }
                            }
                        }
                    }
                }
                Role::Tool => {
                    removed_tool_results += 1;
                }
                Role::System => {}
            }
        }

        if user_goals.is_empty()
            && assistant_findings.is_empty()
            && tool_usage.is_empty()
            && removed_tool_results == 0
            && tool_results_truncated == 0
        {
            return None;
        }

        let mut lines = vec![
            format!(
                "{} Older conversation was compacted to stay within the model window.",
                CONTEXT_SUMMARY_PREFIX
            ),
            format!("- Removed messages: {}", removed_messages.len()),
        ];

        if !user_goals.is_empty() {
            lines.push(format!("- Earlier user goals: {}", user_goals.join(" | ")));
        }

        if !assistant_findings.is_empty() {
            lines.push(format!(
                "- Earlier assistant findings: {}",
                assistant_findings.join(" | ")
            ));
        }

        if !tool_usage.is_empty() {
            let tool_summary = tool_usage
                .iter()
                .take(5)
                .map(|(name, count)| {
                    if *count > 1 {
                        format!("{} x{}", name, count)
                    } else {
                        name.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!("- Earlier tool activity: {}", tool_summary));
        }

        if removed_tool_results > 0 || tool_results_truncated > 0 {
            lines.push(format!(
                "- Tool results compacted: {} removed, {} truncated",
                removed_tool_results, tool_results_truncated
            ));
        }

        let mut summary = lines.join("\n");
        if summary.chars().count() > SUMMARY_CHAR_BUDGET {
            summary = summary
                .chars()
                .take(SUMMARY_CHAR_BUDGET)
                .collect::<String>();
            summary.push_str("...");
        }

        Some(summary)
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
                if let Some(ref content) = msg.content {
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
            let mut min_idx = 1;
            for (i, msg) in messages.iter().enumerate().take(remove_end).skip(1) {
                let p = message_priority(msg);
                if p < min_priority {
                    min_priority = p;
                    min_idx = i;
                }
            }

            let removed_msg = messages.remove(min_idx);
            let role = removed_msg.role.clone();
            removed_messages.push(removed_msg);
            report.removed += 1;

            if matches!(role, Role::Tool) && min_idx > 0 {
                let prev = min_idx - 1;
                if prev < messages.len()
                    && matches!(messages[prev].role, Role::Assistant)
                    && !messages[prev].tool_calls.is_empty()
                {
                    let tc_ids: Vec<String> = messages[prev]
                        .tool_calls
                        .iter()
                        .map(|tc| tc.id.clone())
                        .collect();
                    let has_results = messages.iter().any(|m| {
                        matches!(m.role, Role::Tool)
                            && m.tool_call_id
                                .as_ref()
                                .map(|id| tc_ids.contains(id))
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

    pub fn context_window(&self) -> usize {
        self.limits.context_window
    }

    pub fn compression_threshold_tokens(&self) -> usize {
        (self.limits.context_window as f64 * self.threshold) as usize
    }

    pub fn estimate_tokens_for_messages(&self, messages: &[Message]) -> usize {
        self.estimate_tokens(messages)
    }

    pub fn exceeds_threshold_estimate(&self, messages: &[Message]) -> bool {
        (self.estimate_tokens(messages) as f64)
            > (self.limits.context_window as f64 * self.threshold)
    }
}
