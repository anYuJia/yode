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
            warning_threshold: 0.90,
            auto_compact_threshold: 0.93,
            blocking_threshold: 0.97,
            last_known_prompt_tokens: None,
            last_known_char_count: None,
        }
    }

    pub fn assess_prompt_pressure(
        &mut self,
        prompt_tokens: u32,
        messages: &[Message],
    ) -> ContextPressure {
        self.last_known_prompt_tokens = Some(prompt_tokens);
        let char_count = messages_char_count(messages);
        self.last_known_char_count = Some(char_count);
        self.assess_tokens(prompt_tokens as usize)
    }

    pub fn assess_message_pressure(&self, messages: &[Message]) -> ContextPressure {
        self.assess_tokens(self.estimate_tokens(messages))
    }

    /// Check if the current token usage suggests we should compress.
    pub fn should_compress(&mut self, prompt_tokens: u32, messages: &[Message]) -> bool {
        matches!(
            self.assess_prompt_pressure(prompt_tokens, messages).level,
            ContextPressureLevel::AutoCompact | ContextPressureLevel::Blocking
        )
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

    pub fn warning_threshold_tokens(&self) -> usize {
        (self.limits.context_window as f64 * self.warning_threshold) as usize
    }

    pub fn compression_threshold_tokens(&self) -> usize {
        (self.limits.context_window as f64 * self.auto_compact_threshold) as usize
    }

    pub fn blocking_threshold_tokens(&self) -> usize {
        (self.limits.context_window as f64 * self.blocking_threshold) as usize
    }

    pub fn estimate_tokens_for_messages(&self, messages: &[Message]) -> usize {
        self.estimate_tokens(messages)
    }

    pub fn exceeds_threshold_estimate(&self, messages: &[Message]) -> bool {
        matches!(
            self.assess_message_pressure(messages).level,
            ContextPressureLevel::AutoCompact | ContextPressureLevel::Blocking
        )
    }

    fn assess_tokens(&self, observed_tokens: usize) -> ContextPressure {
        let warning_threshold_tokens = self.warning_threshold_tokens();
        let auto_compact_threshold_tokens = self.compression_threshold_tokens();
        let blocking_threshold_tokens = self.blocking_threshold_tokens();
        let level = if observed_tokens >= blocking_threshold_tokens {
            ContextPressureLevel::Blocking
        } else if observed_tokens >= auto_compact_threshold_tokens {
            ContextPressureLevel::AutoCompact
        } else if observed_tokens >= warning_threshold_tokens {
            ContextPressureLevel::Warning
        } else {
            ContextPressureLevel::Normal
        };

        ContextPressure {
            observed_tokens,
            warning_threshold_tokens,
            auto_compact_threshold_tokens,
            blocking_threshold_tokens,
            level,
        }
    }
}
