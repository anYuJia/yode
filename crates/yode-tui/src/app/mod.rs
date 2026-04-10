pub mod commands;
pub mod completion;
mod engine_events;
pub mod history;
pub mod input;
mod key_dispatch;
mod key_handlers;
mod lifecycle;
mod rendering;
mod runtime;
mod scrollback;
mod turn_flow;
pub mod wizard;

use regex::Regex;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use yode_core::engine::{AgentEngine, ConfirmResponse};
use yode_llm::registry::ProviderRegistry;
use yode_tools::registry::ToolRegistry;
use crate::terminal_caps::TerminalCaps;

use self::completion::{CommandCompletion, FileCompletion};
use self::history::HistoryState;
use crate::app::input::InputState;
pub use self::runtime::run;
pub(crate) use self::scrollback::format_duration;

// ── Content Filtering ───────────────────────────────────────────────

static TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Catch everything from standard tags to malformed snippets and partial results
    Regex::new(r"(?s)\[DUMMY_TOOL_RESULT\]?|\[tool_use\s+[^\]>]+[\]>](?:\s*[:]\s*)?\{.*?\}[\s\]>]*|\[tool_result\s+[^\]>]+[\]>](?:\s*[:]\s*)?\{.*?\}[\s\]>]*|\[tool_(?:use|result)\s+[^\]>]+[\]>]?").unwrap()
});

/// Strips internal protocol tags from assistant text output.
fn strip_internal_tags(text: &str) -> String {
    TAG_RE.replace_all(text, "").to_string()
}

// ── Types ───────────────────────────────────────────────────────────

/// A pending tool confirmation.
#[derive(Debug, Clone)]
pub struct PendingConfirmation {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// Chat display entry.
#[derive(Debug, Clone)]
pub struct ChatEntry {
    pub role: ChatRole,
    pub content: String,
    pub reasoning: Option<String>,
    /// When this entry was created.
    pub timestamp: Instant,
    /// If true, this entry was already printed to scrollback via streaming.
    pub already_printed: bool,
    /// Elapsed time for tool calls (set on ToolResult when matched to ToolCallStart).
    pub duration: Option<Duration>,
    /// Tool execution progress (optional).
    pub progress: Option<yode_tools::tool::ToolProgress>,
    /// Structured tool metadata attached to the result.
    pub tool_metadata: Option<serde_json::Value>,
    /// Structured error type attached to a tool result.
    pub tool_error_type: Option<String>,
}

impl ChatEntry {
    pub fn new(role: ChatRole, content: String) -> Self {
        Self {
            role,
            content,
            reasoning: None,
            timestamp: Instant::now(),
            already_printed: false,
            duration: None,
            progress: None,
            tool_metadata: None,
            tool_error_type: None,
        }
    }

    pub fn new_with_reasoning(role: ChatRole, content: String, reasoning: Option<String>) -> Self {
        Self {
            role,
            content,
            reasoning,
            timestamp: Instant::now(),
            already_printed: false,
            duration: None,
            progress: None,
            tool_metadata: None,
            tool_error_type: None,
        }
    }
}
/// Role for display purposes.
#[derive(Debug, Clone)]
pub enum ChatRole {
    User,
    Assistant,
    ToolCall {
        id: String,
        name: String,
    },
    ToolResult {
        id: String,
        name: String,
        is_error: bool,
    },
    Error,
    System,
    SubAgentCall {
        description: String,
    },
    SubAgentToolCall {
        name: String,
    },
    SubAgentResult,
    AskUser {
        id: String,
    },
}

/// Permission mode for tool execution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PermissionMode {
    Normal,
    AutoAccept,
    Plan,
}

impl PermissionMode {
    pub fn label(&self) -> &'static str {
        match self {
            PermissionMode::Normal => "Normal",
            PermissionMode::AutoAccept => "Auto-Accept",
            PermissionMode::Plan => "Plan",
        }
    }

    pub fn next(self) -> Self {
        match self {
            PermissionMode::Normal => PermissionMode::AutoAccept,
            PermissionMode::AutoAccept => PermissionMode::Plan,
            PermissionMode::Plan => PermissionMode::Normal,
        }
    }
}

// ── Session state ───────────────────────────────────────────────────

/// Persistent session state (model, tokens, etc.)
pub struct SessionState {
    pub model: String,
    pub session_id: String,
    pub working_dir: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    pub previous_prompt_tokens: u32,
    pub tool_call_count: u32,
    pub permission_mode: PermissionMode,
    pub always_allow_tools: Vec<String>,
    /// True when input_tokens is estimated (provider didn't report).
    pub input_estimated: bool,
    /// Tokens used in the current turn only.
    pub turn_input_tokens: u32,
    pub turn_output_tokens: u32,
    /// Resume-time warmup stats for transcript caches, when available.
    pub(crate) resume_cache_warmup:
        Option<crate::commands::info::ResumeTranscriptCacheWarmupStats>,
}

// ── Turn Status Line ───────────────────────────────────────────────
// Unified status: Idle → Working → Done (or Retrying → Working → Done)
// Rendered in a fixed viewport slot above the input separator.

#[derive(Debug, Clone)]
pub enum TurnStatus {
    /// No active turn — status line hidden
    Idle,
    /// LLM is working: `✶ Cogitating… (5s · ↑2539 ↓29 tok)`
    Working { verb: &'static str },
    /// Turn completed: `⚡ Done · 13s · 3 tool calls`
    Done { elapsed: Duration, tools: u32 },
    /// Retrying after error: `⎿ error · Retrying in 3s (2/10)`
    Retrying {
        error: String,
        attempt: u32,
        max_attempts: u32,
        delay_secs: u64,
    },
}

impl TurnStatus {
    pub fn is_visible(&self) -> bool {
        !matches!(self, TurnStatus::Idle)
    }
}

// ── Thinking / Spinner ──────────────────────────────────────────────

const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub struct ThinkingState {
    pub active: bool,
    pub spinner_frame: usize,
    pub started_at: Option<Instant>,
    pub cancel_token: Option<CancellationToken>,
    /// Tick counter to slow down the spinner (advance every N ticks)
    tick_count: usize,
}

/// Fun spinner verbs (inspired by Claude Code)
const SPINNER_VERBS: &[&str] = &[
    "Thinking",
    "Computing",
    "Pondering",
    "Brewing",
    "Crafting",
    "Cooking",
    "Weaving",
    "Forging",
    "Conjuring",
    "Composing",
    "Hatching",
    "Spinning",
    "Churning",
    "Simmering",
    "Percolating",
    "Noodling",
    "Ruminating",
    "Cogitating",
    "Assembling",
    "Channeling",
    "Synthesizing",
    "Crystallizing",
    "Orchestrating",
    "Manifesting",
    "Concocting",
    "Germinating",
    "Incubating",
    "Cultivating",
];

/// Spinner advances every SPINNER_TICK_DIVISOR ticks.
/// Event loop ticks at 50ms, so 4 → one frame per 200ms.
const SPINNER_TICK_DIVISOR: usize = 4;

impl ThinkingState {
    pub fn new() -> Self {
        Self {
            active: false,
            spinner_frame: 0,
            started_at: None,
            cancel_token: None,
            tick_count: 0,
        }
    }

    pub fn start(&mut self, token: CancellationToken) {
        self.active = true;
        self.started_at = Some(Instant::now());
        self.cancel_token = Some(token);
    }

    pub fn stop(&mut self) {
        self.active = false;
        self.started_at = None;
        self.cancel_token = None;
    }

    pub fn cancel(&mut self) {
        if let Some(token) = self.cancel_token.take() {
            token.cancel();
        }
        self.stop();
    }

    pub fn spinner_char(&self) -> char {
        SPINNER_FRAMES[self.spinner_frame % SPINNER_FRAMES.len()]
    }

    pub fn elapsed_secs(&self) -> u64 {
        self.started_at.map(|s| s.elapsed().as_secs()).unwrap_or(0)
    }

    pub fn advance_spinner(&mut self) {
        self.tick_count += 1;
        if self.tick_count >= SPINNER_TICK_DIVISOR {
            self.tick_count = 0;
            self.spinner_frame = (self.spinner_frame + 1) % SPINNER_FRAMES.len();
        }
    }
}

// ── Main App ────────────────────────────────────────────────────────

/// Main application state.
pub struct App {
    // Sub-states
    pub input: InputState,
    pub history: HistoryState,
    pub cmd_completion: CommandCompletion,
    pub file_completion: FileCompletion,
    pub thinking: ThinkingState,
    pub session: SessionState,

    // Chat
    pub chat_entries: Vec<ChatEntry>,
    /// How many entries have been flushed to terminal scrollback.
    pub printed_count: usize,
    /// Current streaming text buffer (assistant response being streamed).
    pub streaming_buf: String,
    pub streaming_reasoning: String,
    /// Partial tag buffer to handle split SSE chunks (e.g. "[tool_u" ... "se]")
    pub streaming_tag_buf: String,
    /// How many lines of streaming_buf have already been printed to scrollback.
    pub streaming_printed_lines: usize,
    /// Whether we're inside a code block during streaming.
    pub streaming_in_code_block: bool,
    /// Unprinted remainder from finalized streaming: (lines, is_first_output)
    pub streaming_remainder: Option<(Vec<String>, bool)>,
    /// Whether we've printed the "Thinking..." indicator to scrollback
    pub thinking_printed: bool,
    /// Whether we've received any ReasoningDelta events (for fallback detection)
    pub received_reasoning_delta: bool,

    // Engine communication
    pub pending_confirmation: Option<PendingConfirmation>,
    pub confirm_tx: Option<mpsc::UnboundedSender<ConfirmResponse>>,
    pub pending_inputs: Vec<(String, String)>,
    /// Whether we are currently executing a turn via the engine
    pub is_processing: bool,

    // Control
    pub should_quit: bool,

    // Backward compat aliases (used by UI renderers)
    pub is_thinking: bool,

    // Ctrl+C handling
    pub last_ctrl_c: Option<Instant>,

    // Tool call timing (id → start instant)
    pub tool_call_starts: HashMap<String, Instant>,

    // Session start time (for exit summary)
    pub session_start: Instant,

    // Turn timing: when the current LLM turn started
    pub turn_started_at: Option<Instant>,
    // Tool calls in current turn (reset each turn)
    pub turn_tool_count: u32,

    // Unified turn status line (Working/Done/Retrying)
    pub turn_status: TurnStatus,

    // Confirmation selection index (0=Yes, 1=Always, 2=No)
    pub confirm_selected: usize,

    // Sub-agent tracking
    pub in_sub_agent: bool,
    pub sub_agent_tool_count: usize,

    // Terminal capabilities
    pub terminal_caps: TerminalCaps,

    // Provider management
    pub provider_name: String,
    pub provider_models: Vec<String>,
    /// Map of provider_name → models list (for switching)
    pub all_provider_models: HashMap<String, Vec<String>>,
    /// Provider registry for runtime switching
    pub provider_registry: Arc<ProviderRegistry>,

    /// Engine reference for hot-reload
    pub engine: Option<Arc<Mutex<AgentEngine>>>,

    /// Tool registry (for completion context)
    pub tools: Arc<ToolRegistry>,

    /// Command registry for slash commands
    pub cmd_registry: crate::commands::registry::CommandRegistry,

    /// Active interactive wizard (multi-step input flow)
    pub wizard: Option<wizard::Wizard>,

    /// Update check result (new version available)
    pub update_available: Option<String>,
    /// Whether update is being downloaded
    pub update_downloading: bool,
    /// Downloaded update path (ready to install)
    pub update_downloaded: Option<String>,

    /// Prompt suggestion (ghost text shown at cursor end when input is empty)
    pub prompt_suggestion: Option<String>,
    /// Whether prompt suggestion is enabled
    pub prompt_suggestion_enabled: bool,
    /// Whether a suggestion is currently being generated (to avoid duplicate requests)
    pub suggestion_generating: bool,
    /// Last time a suggestion was generated (for cooldown)
    pub last_suggestion_time: Instant,
    /// Last time a running background-task brief was surfaced.
    pub last_task_brief_time: Instant,
}

impl App {
    pub fn new(
        model: String,
        session_id: String,
        working_dir: String,
        provider_name: String,
        provider_models: Vec<String>,
        all_provider_models: HashMap<String, Vec<String>>,
        provider_registry: Arc<ProviderRegistry>,
        tools: Arc<ToolRegistry>,
    ) -> Self {
        Self {
            input: InputState::new(),
            history: HistoryState::new(),
            cmd_completion: CommandCompletion::new(),
            file_completion: FileCompletion::new(),
            thinking: ThinkingState::new(),
            session: SessionState {
                model,
                session_id,
                working_dir,
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                previous_prompt_tokens: 0,
                tool_call_count: 0,
                permission_mode: PermissionMode::Normal,
                always_allow_tools: Vec::new(),
                input_estimated: false,
                turn_input_tokens: 0,
                turn_output_tokens: 0,
                resume_cache_warmup: None,
            },
            chat_entries: Vec::new(),
            printed_count: 0,
            streaming_buf: String::new(),
            streaming_reasoning: String::new(),
            streaming_tag_buf: String::new(),
            streaming_printed_lines: 0,
            streaming_in_code_block: false,
            streaming_remainder: None,
            thinking_printed: false,
            received_reasoning_delta: false,
            pending_confirmation: None,
            confirm_tx: None,
            pending_inputs: Vec::new(),
            is_processing: false,
            should_quit: false,
            is_thinking: false,
            last_ctrl_c: None,
            tool_call_starts: HashMap::new(),
            session_start: Instant::now(),
            turn_started_at: None,
            turn_tool_count: 0,
            turn_status: TurnStatus::Idle,
            confirm_selected: 0,
            in_sub_agent: false,
            sub_agent_tool_count: 0,
            terminal_caps: TerminalCaps::detect(),
            provider_name,
            provider_models,
            all_provider_models,
            provider_registry,
            engine: None,
            tools,
            cmd_registry: crate::commands::registry::CommandRegistry::new(),
            wizard: None,
            update_available: None,
            update_downloading: false,
            update_downloaded: None,
            prompt_suggestion: None,
            prompt_suggestion_enabled: true,
            suggestion_generating: false,
            last_suggestion_time: Instant::now(),
            last_task_brief_time: Instant::now(),
        }
    }

    /// Sync is_thinking from thinking state (call after state changes).
    fn sync_thinking(&mut self) {
        self.is_thinking = self.thinking.active;
    }

    /// Cancel current generation.
    fn cancel_generation(&mut self) {
        self.thinking.cancel();
        self.pending_confirmation = None;
        self.sync_thinking();
        self.chat_entries.push(ChatEntry::new(
            ChatRole::System,
            "Generation cancelled.".to_string(),
        ));
    }

    // ── Delegated accessors for UI compatibility ────────────────────

    pub fn spinner_char(&self) -> char {
        self.thinking.spinner_char()
    }

    pub fn thinking_elapsed_secs(&self) -> u64 {
        self.thinking.elapsed_secs()
    }

    pub fn thinking_elapsed_str(&self) -> String {
        let d = self
            .turn_started_at
            .map(|s| s.elapsed())
            .unwrap_or_default();
        format_duration(d)
    }

    pub fn spinner_frame(&self) -> usize {
        self.thinking.spinner_frame
    }

    pub fn input_height(&self, terminal_height: u16) -> u16 {
        self.input.area_height(terminal_height)
    }
}

// ── Skill Command Wrapper ──────────────────────────────────────────

/// Dynamic skill command wrapper that delegates execution via the engine.
struct SkillCommandWrapper {
    meta: crate::commands::CommandMeta,
}

impl crate::commands::Command for SkillCommandWrapper {
    fn meta(&self) -> &crate::commands::CommandMeta {
        &self.meta
    }

    fn execute(
        &self,
        _args: &str,
        _ctx: &mut crate::commands::context::CommandContext,
    ) -> crate::commands::CommandResult {
        // Skill commands are handled by showing the skill description;
        // actual execution flows through the normal chat/engine path.
        Ok(crate::commands::CommandOutput::Message(format!(
            "Skill command: {}",
            self.meta.description
        )))
    }
}

// SAFETY: SkillCommandWrapper holds only static references and is safe to share.
unsafe impl Send for SkillCommandWrapper {}
unsafe impl Sync for SkillCommandWrapper {}

/// Find substring case-insensitively, return byte offset
fn find_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    haystack.to_lowercase().find(&needle.to_lowercase())
}

fn push_grouped_system_entry(app: &mut App, group_prefix: &str, content: String) {
    if let Some(last) = app.chat_entries.last_mut() {
        if matches!(last.role, ChatRole::System)
            && last.content.starts_with(group_prefix)
            && last.timestamp.elapsed() <= Duration::from_secs(5)
        {
            if !last.content.contains(&content) {
                last.content.push('\n');
                last.content.push_str(&content);
            }
            return;
        }
    }
    app.chat_entries
        .push(ChatEntry::new(ChatRole::System, content));
}

// ── Scrollback printing ─────────────────────────────────────────────
