pub(crate) mod timeouts {
    pub(crate) const LLM_REQUEST_SECS: u64 = 120;
    pub(crate) const STREAMING_TURN_HARD_SECS: u64 = 600;
    pub(crate) const STREAMING_STALL_SECS: u64 = 120;
    pub(crate) const STREAMING_HEARTBEAT_SECS: u64 = 2;
    pub(crate) const TOOL_EXECUTION_SECS: u64 = 120;
    pub(crate) const PARALLEL_TOOL_SECS: u64 = 30;
    pub(crate) const LLM_COMPACTION_SUMMARY_SECS: u64 = 30;
    pub(crate) const PROMPT_SUGGESTION_SECS: u64 = 5;
    pub(crate) const TOOL_CONFIRMATION_SECS: u64 = 90;
    pub(crate) const TOOL_CONFIRMATION_POLL_MS: u64 = 500;
}

pub(crate) mod thresholds {
    pub(crate) const TOOL_BUDGET_NOTICE: u32 = 15;
    pub(crate) const TOOL_BUDGET_WARNING: u32 = 25;
    pub(crate) const TOOL_REFLECT_INTERVAL: u32 = 10;
    pub(crate) const TOOL_GOAL_REMINDER: u32 = 5;
    pub(crate) const MAX_CONSECUTIVE_COMPACTION_FAILURES: u32 = 3;
    pub(crate) const SESSION_MEMORY_INIT_CHARS: usize = 8_000;
    pub(crate) const SESSION_MEMORY_CHAR_DELTA: usize = 4_000;
    pub(crate) const SESSION_MEMORY_TOOL_DELTA: u32 = 3;
}

const _: () = {
    assert!(timeouts::STREAMING_TURN_HARD_SECS >= timeouts::LLM_REQUEST_SECS);
    assert!(timeouts::STREAMING_STALL_SECS <= timeouts::STREAMING_TURN_HARD_SECS);
    assert!(timeouts::STREAMING_HEARTBEAT_SECS < timeouts::STREAMING_STALL_SECS);
    assert!(timeouts::PARALLEL_TOOL_SECS < timeouts::LLM_REQUEST_SECS);
    assert!(timeouts::PARALLEL_TOOL_SECS < timeouts::TOOL_EXECUTION_SECS);
    assert!(timeouts::PROMPT_SUGGESTION_SECS < timeouts::LLM_REQUEST_SECS);
    assert!(timeouts::LLM_COMPACTION_SUMMARY_SECS < timeouts::LLM_REQUEST_SECS);
    assert!(timeouts::TOOL_CONFIRMATION_SECS < timeouts::TOOL_EXECUTION_SECS);
    assert!(timeouts::TOOL_CONFIRMATION_POLL_MS < 1_000);
    assert!(thresholds::TOOL_BUDGET_NOTICE < thresholds::TOOL_BUDGET_WARNING);
    assert!(thresholds::SESSION_MEMORY_CHAR_DELTA < thresholds::SESSION_MEMORY_INIT_CHARS);
};
