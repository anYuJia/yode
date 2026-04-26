mod runtime;

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
    /// Warning threshold as a fraction of context_window.
    warning_threshold: f64,
    /// Auto-compaction threshold as a fraction of context_window.
    auto_compact_threshold: f64,
    /// Hard blocking threshold as a fraction of context_window.
    blocking_threshold: f64,
    /// Cached prompt_tokens from last API response for better estimation.
    last_known_prompt_tokens: Option<u32>,
    /// Cached char count at the time prompt_tokens was recorded.
    last_known_char_count: Option<usize>,
}

/// Number of recent messages to always preserve during compression.
const PRESERVE_RECENT: usize = 6;
/// Maximum characters for truncated tool results during compression.
const COMPRESSED_TOOL_RESULT_MAX: usize = 500;
/// Preserve a slightly larger live tail for per-turn microcompact.
const MICROCOMPACT_PRESERVE_RECENT: usize = 8;
/// Tool results below this size are not worth clearing during microcompact.
const MICROCOMPACT_TRIGGER_CHARS: usize = 320;
/// Preview budget for tool-result placeholders inserted by microcompact.
const MICROCOMPACT_PREVIEW_CHARS: usize = 160;
/// Keep generated compression summaries compact so they do not immediately bloat context again.
const SUMMARY_CHAR_BUDGET: usize = 1_200;
const CONTEXT_SUMMARY_PREFIX: &str = "[Context summary]";
const MICROCOMPACT_PREFIX: &str = "[Tool result microcompacted]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextPressureLevel {
    Normal,
    Warning,
    AutoCompact,
    Blocking,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextPressure {
    pub observed_tokens: usize,
    pub warning_threshold_tokens: usize,
    pub auto_compact_threshold_tokens: usize,
    pub blocking_threshold_tokens: usize,
    pub level: ContextPressureLevel,
}

#[derive(Debug, Clone, Default)]
pub struct CompressionReport {
    pub removed: usize,
    pub tool_results_truncated: usize,
    pub summary: Option<String>,
    pub removed_messages: Vec<Message>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MicrocompactReport {
    pub tool_results_cleared: usize,
    pub saved_chars: usize,
}

#[cfg(test)]
mod tests;
