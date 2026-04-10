use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use yode_core::engine::{AgentEngine, ConfirmResponse};
use yode_llm::registry::ProviderRegistry;
use yode_tools::registry::ToolRegistry;

use crate::terminal_caps::TerminalCaps;

use super::completion::{CommandCompletion, FileCompletion};
use super::history::HistoryState;
use super::input::InputState;
use super::scrollback::format_duration;
use super::wizard;

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

// Unified status: Idle -> Working -> Done (or Retrying -> Working -> Done)
// Rendered in a fixed viewport slot above the input separator.
#[derive(Debug, Clone)]
pub enum TurnStatus {
    Idle,
    Working { verb: &'static str },
    Done { elapsed: Duration, tools: u32 },
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

const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub(crate) const SPINNER_VERBS: &[&str] = &[
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
/// Event loop ticks at 50ms, so 4 -> one frame per 200ms.
const SPINNER_TICK_DIVISOR: usize = 4;

pub struct ThinkingState {
    pub active: bool,
    pub spinner_frame: usize,
    pub started_at: Option<Instant>,
    pub cancel_token: Option<CancellationToken>,
    tick_count: usize,
}

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

/// Main application state.
pub struct App {
    pub input: InputState,
    pub history: HistoryState,
    pub cmd_completion: CommandCompletion,
    pub file_completion: FileCompletion,
    pub thinking: ThinkingState,
    pub session: SessionState,
    pub chat_entries: Vec<ChatEntry>,
    pub printed_count: usize,
    pub streaming_buf: String,
    pub streaming_reasoning: String,
    pub streaming_tag_buf: String,
    pub streaming_printed_lines: usize,
    pub streaming_in_code_block: bool,
    pub streaming_remainder: Option<(Vec<String>, bool)>,
    pub thinking_printed: bool,
    pub received_reasoning_delta: bool,
    pub pending_confirmation: Option<PendingConfirmation>,
    pub confirm_tx: Option<mpsc::UnboundedSender<ConfirmResponse>>,
    pub pending_inputs: Vec<(String, String)>,
    pub is_processing: bool,
    pub should_quit: bool,
    pub is_thinking: bool,
    pub last_ctrl_c: Option<Instant>,
    pub tool_call_starts: HashMap<String, Instant>,
    pub session_start: Instant,
    pub turn_started_at: Option<Instant>,
    pub turn_tool_count: u32,
    pub turn_status: TurnStatus,
    pub confirm_selected: usize,
    pub in_sub_agent: bool,
    pub sub_agent_tool_count: usize,
    pub terminal_caps: TerminalCaps,
    pub provider_name: String,
    pub provider_models: Vec<String>,
    pub all_provider_models: HashMap<String, Vec<String>>,
    pub provider_registry: Arc<ProviderRegistry>,
    pub engine: Option<Arc<Mutex<AgentEngine>>>,
    pub tools: Arc<ToolRegistry>,
    pub cmd_registry: crate::commands::registry::CommandRegistry,
    pub wizard: Option<wizard::Wizard>,
    pub update_available: Option<String>,
    pub update_downloading: bool,
    pub update_downloaded: Option<String>,
    pub prompt_suggestion: Option<String>,
    pub prompt_suggestion_enabled: bool,
    pub suggestion_generating: bool,
    pub last_suggestion_time: Instant,
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

    pub(super) fn sync_thinking(&mut self) {
        self.is_thinking = self.thinking.active;
    }

    pub(super) fn cancel_generation(&mut self) {
        self.thinking.cancel();
        self.pending_confirmation = None;
        self.sync_thinking();
        self.chat_entries.push(ChatEntry::new(
            ChatRole::System,
            "Generation cancelled.".to_string(),
        ));
    }

    pub fn spinner_char(&self) -> char {
        self.thinking.spinner_char()
    }

    pub fn thinking_elapsed_secs(&self) -> u64 {
        self.thinking.elapsed_secs()
    }

    pub fn thinking_elapsed_str(&self) -> String {
        let d = self.turn_started_at.map(|s| s.elapsed()).unwrap_or_default();
        format_duration(d)
    }

    pub fn spinner_frame(&self) -> usize {
        self.thinking.spinner_frame
    }

    pub fn input_height(&self, terminal_height: u16) -> u16 {
        self.input.area_height(terminal_height)
    }
}
