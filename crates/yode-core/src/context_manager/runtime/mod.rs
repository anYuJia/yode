mod compression;
mod summary;

use super::*;

#[allow(unused_imports)]
pub(crate) use compression::{is_context_summary, message_priority};
#[allow(unused_imports)]
pub(crate) use summary::{calibration_token_estimate, context_summary_lines, messages_char_count};

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
        let char_count = messages_char_count(messages);
        self.last_known_char_count = Some(char_count);
        (prompt_tokens as f64) > (self.limits.context_window as f64 * self.threshold)
    }

    /// Estimate token count for the given messages.
    pub(in crate::context_manager) fn estimate_tokens(&self, messages: &[Message]) -> usize {
        calibration_token_estimate(
            messages_char_count(messages),
            self.last_known_prompt_tokens,
            self.last_known_char_count,
        )
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
