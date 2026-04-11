mod runtime;

use std::collections::BTreeMap;

use yode_llm::types::{Message, Role};

/// Known model context window sizes.
#[derive(Debug, Clone)]
pub struct ModelLimits {
    /// Maximum context window in tokens.
    pub context_window: usize,
    /// Maximum output tokens per response.
    pub output_tokens: usize,
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

#[cfg(test)]
mod tests;
