mod types;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{mpsc, Mutex};

use yode_core::engine::{AgentEngine, ConfirmResponse};
use yode_llm::registry::ProviderRegistry;
use yode_tools::registry::ToolRegistry;

use crate::terminal_caps::TerminalCaps;

use super::completion::{CommandCompletion, FileCompletion};
use super::history::HistoryState;
use super::input::InputState;
use super::scrollback::format_duration;
use super::wizard;

pub use self::types::{
    ChatEntry, ChatRole, PendingConfirmation, PermissionMode, SessionState, ThinkingState,
    TurnStatus,
};
pub(crate) use self::types::SPINNER_VERBS;

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
                startup_profile: None,
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
        let duration = self.turn_started_at.map(|start| start.elapsed()).unwrap_or_default();
        format_duration(duration)
    }

    pub fn spinner_frame(&self) -> usize {
        self.thinking.spinner_frame
    }

    pub fn input_height(&self, terminal_height: u16) -> u16 {
        self.input.area_height(terminal_height)
    }
}
