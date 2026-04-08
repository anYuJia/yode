use std::collections::BTreeMap;

use tracing::info;
use yode_llm::types::{Message, Role};

/// Known model context window sizes.
#[derive(Debug, Clone)]
pub struct ModelLimits {
    /// Maximum context window in tokens.
    pub context_window: usize,
    /// Maximum output tokens per response.
    pub output_tokens: usize,
}

impl ModelLimits {
    /// Look up known model limits by model name.
    pub fn for_model(model: &str) -> Self {
        let model_lower = model.to_lowercase();
        if model_lower.contains("claude-sonnet-4") || model_lower.contains("claude-3-5-sonnet") {
            Self {
                context_window: 200_000,
                output_tokens: 8_192,
            }
        } else if model_lower.contains("claude-opus") || model_lower.contains("claude-3-opus") {
            Self {
                context_window: 200_000,
                output_tokens: 4_096,
            }
        } else if model_lower.contains("claude-haiku") || model_lower.contains("claude-3-haiku") {
            Self {
                context_window: 200_000,
                output_tokens: 4_096,
            }
        } else if model_lower.contains("gpt-4o") {
            Self {
                context_window: 128_000,
                output_tokens: 4_096,
            }
        } else if model_lower.contains("gpt-4-turbo") {
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
            // Conservative default
            Self {
                context_window: 128_000,
                output_tokens: 4_096,
            }
        }
    }
}

/// Manages context window usage to prevent token limit overflows.
pub struct ContextManager {
    limits: ModelLimits,
    /// Compression threshold as a fraction of context_window (default: 0.75).
    threshold: f64,
    /// Cached prompt_tokens from last API response for better estimation.
    last_known_prompt_tokens: Option<u32>,
    /// Cached char count at the time prompt_tokens was recorded.
    last_known_char_count: Option<usize>,
}

/// Number of recent messages to always preserve during compression.
const PRESERVE_RECENT: usize = 6;
/// Maximum characters for truncated tool results during compression.
const COMPRESSED_TOOL_RESULT_MAX: usize = 500;
/// Keep generated compression summaries compact so they do not immediately bloat context again.
const SUMMARY_CHAR_BUDGET: usize = 1_200;
const CONTEXT_SUMMARY_PREFIX: &str = "[Context summary]";

#[derive(Debug, Clone, Default)]
pub struct CompressionReport {
    pub removed: usize,
    pub tool_results_truncated: usize,
    pub summary: Option<String>,
}

fn is_context_summary(msg: &Message) -> bool {
    matches!(msg.role, Role::System)
        && msg
            .content
            .as_deref()
            .unwrap_or_default()
            .starts_with(CONTEXT_SUMMARY_PREFIX)
}

/// Message removal priority (lower = removed first).
fn message_priority(msg: &Message) -> u32 {
    if is_context_summary(msg) {
        return 2;
    }

    match msg.role {
        Role::System => 99,
        Role::Assistant if !msg.tool_calls.is_empty() => 3,
        Role::Tool => 2,
        _ => 1, // plain user/assistant text
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
    /// Also caches the prompt_tokens and char count for better estimation during compression.
    pub fn should_compress(&mut self, prompt_tokens: u32, messages: &[Message]) -> bool {
        self.last_known_prompt_tokens = Some(prompt_tokens);
        // Compute and cache char count for current messages
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
    /// Uses the cached prompt_tokens and char count to compute a per-char token ratio,
    /// then applies it to the current char count.
    fn estimate_tokens(&self, messages: &[Message]) -> usize {
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
                    // Use the real ratio: tokens_per_char = known_tokens / known_chars
                    return ((char_count as f64) * (known_tokens as f64 / known_chars as f64))
                        as usize;
                }
            }
        }

        // Fallback: rough estimate
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

    /// Compress the message history to reduce token usage.
    ///
    /// Strategy:
    /// 1. Always preserve the system message (index 0) and the last `PRESERVE_RECENT` messages.
    /// 2. In the middle section, truncate tool result messages to `COMPRESSED_TOOL_RESULT_MAX` chars.
    /// 3. If still over threshold (estimated), remove lowest-priority messages from the middle first.
    ///
    /// Returns a report describing what was compacted.
    pub fn compress_with_report(&self, messages: &mut Vec<Message>) -> CompressionReport {
        let original_len = messages.len();
        let mut report = CompressionReport::default();

        if messages.len() <= PRESERVE_RECENT + 1 {
            return report;
        }

        let preserve_start = 1; // After system message
        let preserve_end = messages.len().saturating_sub(PRESERVE_RECENT);
        let mut removed_messages = Vec::new();

        // Phase 1: Truncate tool results in the middle section
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

        // Phase 2: Estimate if we need to remove messages
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

        // Phase 3: Remove lowest-priority messages from the middle.
        // Remove in pairs to avoid orphaned tool_calls/results:
        // - Removing a Tool message also removes the preceding Assistant (if it had tool_calls).
        // - Removing an Assistant with tool_calls also removes subsequent Tool results.
        while messages.len() > PRESERVE_RECENT + 1 {
            let current_estimate = self.estimate_tokens(messages);
            if current_estimate <= target_tokens {
                break;
            }

            let remove_end = messages.len().saturating_sub(PRESERVE_RECENT);
            if remove_end <= 1 {
                break;
            }

            // Find the lowest-priority message in the removable range [1..remove_end)
            let mut min_priority = u32::MAX;
            let mut min_idx = 1;
            for i in 1..remove_end {
                let p = message_priority(&messages[i]);
                if p < min_priority {
                    min_priority = p;
                    min_idx = i;
                }
            }

            // Remove the message and any orphaned pair
            let removed_msg = messages.remove(min_idx);
            let role = removed_msg.role.clone();
            removed_messages.push(removed_msg);
            report.removed += 1;

            // If we removed a Tool result, check if the previous message was an Assistant
            // with tool_calls that now has no results — remove it too
            if matches!(role, Role::Tool) && min_idx > 0 {
                let prev = min_idx - 1;
                if prev < messages.len()
                    && matches!(messages[prev].role, Role::Assistant)
                    && !messages[prev].tool_calls.is_empty()
                {
                    // Check if any tool results still reference this assistant's tool_calls
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

    /// Backward-compatible helper returning only the number of removed messages.
    pub fn compress(&self, messages: &mut Vec<Message>) -> usize {
        self.compress_with_report(messages).removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yode_llm::types::Message;

    #[test]
    fn test_model_limits_lookup() {
        let limits = ModelLimits::for_model("claude-sonnet-4-20250514");
        assert_eq!(limits.context_window, 200_000);

        let limits = ModelLimits::for_model("gpt-4o");
        assert_eq!(limits.context_window, 128_000);

        let limits = ModelLimits::for_model("unknown-model");
        assert_eq!(limits.context_window, 128_000); // default
    }

    #[test]
    fn test_should_compress() {
        let mut cm = ContextManager::new("claude-sonnet-4");
        let msgs = vec![Message::user("hello")];
        // 200k * 0.75 = 150k threshold
        assert!(!cm.should_compress(100_000, &msgs));
        assert!(cm.should_compress(160_000, &msgs));
        // Verify caching
        assert_eq!(cm.last_known_prompt_tokens, Some(160_000));
    }

    #[test]
    fn test_compress_few_messages_noop() {
        let cm = ContextManager::new("claude-sonnet-4");
        let mut messages = vec![
            Message::system("system"),
            Message::user("hello"),
            Message::assistant("hi"),
        ];
        let removed = cm.compress(&mut messages);
        assert_eq!(removed, 0);
        assert_eq!(messages.len(), 3);
    }

    #[test]
    fn test_compress_truncates_tool_results() {
        let cm = ContextManager::new("claude-sonnet-4");
        let long_content = "x".repeat(1000);
        let mut messages = vec![
            Message::system("system"),
            Message::user("q1"),
            Message::assistant("a1"),
            Message::tool_result("tc1", &long_content),
            Message::user("q2"),
            Message::assistant("a2"),
            Message::tool_result("tc2", &long_content),
            Message::user("q3"),
            Message::assistant("a3"),
            // Last 6: these are preserved
            Message::user("q4"),
            Message::assistant("a4"),
            Message::user("q5"),
            Message::assistant("a5"),
            Message::user("q6"),
            Message::assistant("a6"),
        ];
        let report = cm.compress_with_report(&mut messages);
        assert_eq!(report.tool_results_truncated, 2);
        // Tool results in the middle should be truncated
        if let Some(ref content) = messages[3].content {
            assert!(content.len() < 1000);
            assert!(content.contains("[compressed]"));
        }
    }

    #[test]
    fn test_message_priority() {
        assert_eq!(message_priority(&Message::system("sys")), 99);
        assert_eq!(message_priority(&Message::user("hi")), 1);
        assert_eq!(message_priority(&Message::assistant("ok")), 1);
        assert_eq!(message_priority(&Message::tool_result("id", "res")), 2);
        assert_eq!(
            message_priority(&Message::system("[Context summary] previous turns")),
            2
        );
    }

    #[test]
    fn test_estimate_tokens_without_cache() {
        let cm = ContextManager::new("claude-sonnet-4");
        // 400 chars / 4 = 100 tokens
        let messages = vec![Message::user(&"x".repeat(400))];
        assert_eq!(cm.estimate_tokens(&messages), 100);
    }

    #[test]
    fn test_estimate_tokens_with_cache_scales() {
        let mut cm = ContextManager::new("claude-sonnet-4");
        // Cache: 10k tokens seen when char count was 1000
        let baseline = vec![Message::user(&"x".repeat(1000))];
        cm.should_compress(10_000, &baseline); // cache 10k tokens for 1000 chars

        // Same char count → same token estimate
        let messages = vec![Message::user(&"x".repeat(1000))];
        let est = cm.estimate_tokens(&messages);
        assert_eq!(est, 10_000);

        // Half the chars → half the tokens
        let messages = vec![Message::user(&"x".repeat(500))];
        let est = cm.estimate_tokens(&messages);
        assert_eq!(est, 5_000);

        // Double the chars → double the tokens
        let messages = vec![Message::user(&"x".repeat(2000))];
        let est = cm.estimate_tokens(&messages);
        assert_eq!(est, 20_000);
    }

    #[test]
    fn test_compress_removes_low_priority_first() {
        // Build a scenario where phase 3 kicks in.
        // gpt-3.5 = 16k window, target = 16k * 0.6 = 9.6k tokens ≈ 38.4k chars
        // We need estimated tokens > 9.6k → chars > 38.4k
        let cm = ContextManager::new("gpt-3.5");
        let big = "x".repeat(15_000); // each msg ~15k chars → ~3750 tokens
        let mut messages = vec![
            Message::system("system"),
            Message::user(&big),              // priority 1
            Message::tool_result("t1", &big), // priority 2
            Message::assistant(&big),         // priority 1
            Message::user(&big),              // priority 1
            Message::assistant(&big),         // priority 1
            Message::user(&big),              // priority 1
            Message::assistant(&big),         // priority 1
            // Last 6 preserved:
            Message::user("recent1"),
            Message::assistant("recent2"),
            Message::user("recent3"),
            Message::assistant("recent4"),
            Message::user("recent5"),
            Message::assistant("recent6"),
        ];
        let original_len = messages.len();
        let report = cm.compress_with_report(&mut messages);
        assert!(
            report.removed > 0,
            "Expected some messages to be removed, but none were"
        );
        assert!(messages.len() <= original_len);
        assert!(messages.iter().any(is_context_summary));
        // System message always preserved
        assert!(matches!(messages[0].role, Role::System));
    }

    #[test]
    fn test_compression_stress_realistic_conversation() {
        // Simulate a 50+ message conversation like a real coding session:
        // system → (user → assistant_with_tool_calls → tool_result)* → user → assistant_text
        let mut cm = ContextManager::new("gpt-3.5"); // 16k window to trigger compression easily

        let mut messages = vec![Message::system(&"You are a coding assistant. ".repeat(100))]; // ~2.6k chars

        // 15 turns of: user question → assistant tool call → tool result → assistant answer
        for i in 0..15 {
            messages.push(Message::user(&format!(
                "Please read file{}.rs and explain it",
                i
            )));

            // Assistant with tool call
            let mut assistant = Message::assistant(&format!("Let me read file{}.rs for you.", i));
            assistant.tool_calls.push(yode_llm::types::ToolCall {
                id: format!("tc_{}", i),
                name: "read_file".to_string(),
                arguments: format!("{{\"path\": \"file{}.rs\"}}", i),
            });
            messages.push(assistant);

            // Tool result (simulating file content — big enough to force compression)
            messages.push(Message::tool_result(
                &format!("tc_{}", i),
                &format!(
                    "// file{}.rs\n{}",
                    i,
                    "fn example() { /* lots of code here */ }\n".repeat(100)
                ),
            ));

            // Assistant explanation
            messages.push(Message::assistant(&format!(
                "File{}.rs contains an example function that {}. The implementation is straightforward.",
                i, "x".repeat(200)
            )));
        }

        let original_len = messages.len();
        assert!(
            original_len > 50,
            "Should have 60+ messages, got {}",
            original_len
        );

        // Simulate API reporting high token usage so compression triggers.
        // Total chars in our messages is large; set cached tokens high enough to exceed target after scaling.
        let total_chars: usize = messages
            .iter()
            .map(|m| {
                m.content.as_ref().map(|c| c.len()).unwrap_or(0)
                    + m.tool_calls
                        .iter()
                        .map(|tc| tc.arguments.len() + tc.name.len())
                        .sum::<usize>()
            })
            .sum();
        // Cache token count with the current messages for proper ratio estimation.
        let fake_prompt_tokens = (total_chars / 2) as u32;
        cm.should_compress(fake_prompt_tokens, &messages);

        let report = cm.compress_with_report(&mut messages);

        // Should have compressed significantly
        assert!(
            report.removed > 0,
            "Stress test should trigger compression (fake_tokens={}, total_chars={})",
            fake_prompt_tokens,
            total_chars
        );

        // System message always preserved
        assert!(matches!(messages[0].role, Role::System));

        // Last PRESERVE_RECENT messages preserved
        let last_msgs: Vec<_> = messages.iter().rev().take(PRESERVE_RECENT).collect();
        assert_eq!(last_msgs.len(), PRESERVE_RECENT);

        // Tool results in middle should be truncated or messages removed
        let truncated_count = messages
            .iter()
            .filter(|m| {
                matches!(m.role, Role::Tool)
                    && m.content
                        .as_ref()
                        .map(|c| c.contains("[compressed]"))
                        .unwrap_or(false)
            })
            .count();
        assert!(
            truncated_count > 0 || report.removed > 5,
            "Expected truncated tool results or significant removal. truncated={}, removed={}",
            truncated_count,
            report.removed
        );
        assert!(messages.iter().any(is_context_summary));
    }

    #[test]
    fn test_compression_preserves_message_integrity() {
        // Verify no messages get corrupted during compression
        let cm = ContextManager::new("gpt-3.5");
        let mut messages = vec![
            Message::system("SYSTEM_MARKER"),
            Message::user(&"u".repeat(10_000)),
            Message::assistant(&"a".repeat(10_000)),
            Message::tool_result("t1", &"r".repeat(10_000)),
            Message::user(&"u2".repeat(5_000)),
            Message::assistant(&"a2".repeat(5_000)),
            Message::user(&"u3".repeat(5_000)),
            Message::assistant(&"a3".repeat(5_000)),
            Message::user("final_user"),
            Message::assistant("final_assistant"),
            Message::user("last1"),
            Message::assistant("last2"),
            Message::user("last3"),
            Message::assistant("last4"),
        ];

        let report = cm.compress_with_report(&mut messages);
        assert!(report.removed > 0 || report.tool_results_truncated > 0);

        // System message must be first and intact
        assert_eq!(messages[0].content.as_deref(), Some("SYSTEM_MARKER"));

        // All remaining messages must have valid roles
        for msg in &messages {
            assert!(matches!(
                msg.role,
                Role::System | Role::User | Role::Assistant | Role::Tool
            ));
        }

        // No empty content on non-tool messages (tool messages can have None content)
        for msg in &messages {
            if !matches!(msg.role, Role::Tool) {
                assert!(
                    msg.content.is_some(),
                    "Non-tool message has None content: {:?}",
                    msg.role
                );
            }
        }
    }

    #[test]
    fn test_compression_inserts_summary_anchor() {
        let mut cm = ContextManager::new("gpt-3.5");
        let big = "y".repeat(18_000);
        let mut messages = vec![
            Message::system("system"),
            Message::user("Investigate the updater failure on macOS"),
            Message::assistant("I will inspect updater extraction and retry handling."),
            Message::tool_result("tc1", &big),
            Message::assistant("The archive unpack fails under sandboxed temp directories."),
            Message::user(&big),
            Message::assistant("I will compact the earlier findings."),
            Message::user("recent1"),
            Message::assistant("recent2"),
            Message::user("recent3"),
            Message::assistant("recent4"),
            Message::user("recent5"),
            Message::assistant("recent6"),
        ];

        let total_chars: usize = messages
            .iter()
            .map(|m| {
                m.content.as_ref().map(|c| c.len()).unwrap_or(0)
                    + m.tool_calls
                        .iter()
                        .map(|tc| tc.arguments.len() + tc.name.len())
                        .sum::<usize>()
            })
            .sum();
        cm.should_compress(total_chars as u32, &messages);

        let report = cm.compress_with_report(&mut messages);
        assert!(report.removed > 0);
        let summary = report.summary.expect("summary anchor should be inserted");
        assert!(summary.starts_with(CONTEXT_SUMMARY_PREFIX));
        assert!(messages.iter().any(is_context_summary));
    }
}
