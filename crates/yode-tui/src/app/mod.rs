pub mod commands;
pub mod completion;
mod engine_events;
pub mod history;
pub mod input;
mod key_handlers;
mod rendering;
mod scrollback;
mod turn_flow;
pub mod wizard;

use regex::Regex;
use std::collections::HashMap;
use std::io;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use yode_core::context::AgentContext;
use yode_core::db::Database;
use yode_core::engine::{AgentEngine, ConfirmResponse, EngineEvent};
use yode_core::permission::PermissionManager;
use yode_llm::provider::LlmProvider;
use yode_llm::registry::ProviderRegistry;
use yode_llm::types::Message;
use yode_tools::registry::ToolRegistry;

use crate::event::{self, AppEvent};
use crate::terminal_caps::TerminalCaps;
use crate::ui;

use self::completion::{CommandCompletion, FileCompletion};
use self::engine_events::{handle_engine_event, reload_provider_from_config};
use self::history::HistoryState;
use self::key_handlers::{handle_char, handle_down, handle_tab, handle_up};
use self::scrollback::{
    flush_entries_to_scrollback, print_entries_to_stdout, print_header_to_stdout,
};
use self::turn_flow::{handle_enter, try_process_next};
use crate::app::input::InputState;
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

// ── Run TUI ─────────────────────────────────────────────────────────

/// Run the TUI application.
pub async fn run(
    provider: Arc<dyn LlmProvider>,
    provider_registry: Arc<ProviderRegistry>,
    tools: Arc<ToolRegistry>,
    permissions: PermissionManager,
    context: AgentContext,
    db: Database,
    restored_messages: Option<Vec<Message>>,
    skill_commands: Vec<(String, String)>,
    all_provider_models: HashMap<String, Vec<String>>,
) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnableBracketedPaste)?;
    // Add blank line to separate from cargo build output
    stdout.execute(crossterm::style::Print("\n"))?;
    let working_dir = context.working_dir_compat().display().to_string();
    let is_resumed = context.is_resumed;
    let provider_name = context.provider.clone();
    let provider_models = all_provider_models
        .get(&provider_name)
        .cloned()
        .unwrap_or_default();
    let mut app = App::new(
        context.model.clone(),
        context.session_id.clone(),
        working_dir,
        provider_name,
        provider_models,
        all_provider_models,
        provider_registry,
        tools.clone(),
    );
    if is_resumed {
        app.session.resume_cache_warmup = Some(crate::commands::info::warm_resume_transcript_caches(
            &context.working_dir_compat(),
        ));
    }
    app.cmd_completion.dynamic_commands = skill_commands.clone();

    // Register all built-in commands
    crate::commands::register_all(&mut app.cmd_registry);

    // Register dynamic skill commands as simple wrappers
    for (name, description) in &skill_commands {
        app.cmd_registry.register(Box::new(SkillCommandWrapper {
            meta: crate::commands::CommandMeta {
                name: Box::leak(name.clone().into_boxed_str()),
                description: Box::leak(description.clone().into_boxed_str()),
                aliases: &[],
                args: vec![],
                category: crate::commands::CommandCategory::Utility,
                hidden: false,
            },
        }));
    }

    // Print welcome header directly to stdout before starting TUI viewport
    print_header_to_stdout(&app)?;

    // Restore messages to app state if resuming
    if let Some(ref messages) = restored_messages {
        for msg in messages {
            match msg.role {
                yode_llm::types::Role::User => {
                    if let Some(ref content) = msg.content {
                        app.chat_entries
                            .push(ChatEntry::new(ChatRole::User, content.clone()));
                    }
                }
                yode_llm::types::Role::Assistant => {
                    if let Some(ref content) = msg.content {
                        app.chat_entries
                            .push(ChatEntry::new(ChatRole::Assistant, content.clone()));
                    }
                }
                _ => {}
            }
        }
    }

    // Print restored chat entries to stdout
    print_entries_to_stdout(&mut app)?;

    let mut engine_inner = AgentEngine::new(provider, tools.clone(), permissions, context);
    engine_inner.set_database(db);

    if let Ok(config) = yode_core::config::Config::load() {
        if !config.hooks.hooks.is_empty() {
            use yode_core::hooks::{HookDefinition, HookManager};
            let mut hook_mgr = HookManager::new(
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
            );
            for h in &config.hooks.hooks {
                hook_mgr.register(HookDefinition {
                    command: h.command.clone(),
                    events: h.events.clone(),
                    tool_filter: h.tool_filter.clone(),
                    timeout_secs: h.timeout_secs,
                    can_block: h.can_block,
                });
            }
            engine_inner.set_hook_manager(hook_mgr);
        }
    }

    // Restore messages to engine (for context)
    if let Some(ref messages) = restored_messages {
        engine_inner.restore_messages(messages.clone());
        if is_resumed {
            app.chat_entries.push(ChatEntry::new(
                ChatRole::System,
                "Session resumed.".to_string(),
            ));
        }
    }
    engine_inner
        .initialize_session_hooks(if is_resumed { "resume" } else { "startup" })
        .await;

    let engine = Arc::new(Mutex::new(engine_inner));
    app.engine = Some(engine.clone());
    let (engine_event_tx, mut engine_event_rx) = mpsc::unbounded_channel::<EngineEvent>();

    // Check for updates on startup (in background, don't block)
    let update_event_tx = engine_event_tx.clone();
    tokio::spawn(async move {
        let config = match yode_core::config::Config::load() {
            Ok(c) => c,
            Err(_) => return,
        };

        if !config.update.auto_check {
            return;
        }

        let config_dir = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".yode");
        let updater = yode_core::updater::Updater::new(
            config_dir,
            config.update.auto_check,
            config.update.auto_download,
        );

        match updater.check_for_updates().await {
            Ok(Some(result)) => {
                let latest = result.latest_version.clone();
                let _ = update_event_tx.send(EngineEvent::UpdateAvailable(latest.clone()));
                // Auto-download if enabled
                if config.update.auto_download {
                    let _ = update_event_tx.send(EngineEvent::UpdateDownloading);
                    match updater.download_update(&result).await {
                        Ok(path) => {
                            tracing::info!("Update downloaded to: {:?}", path);
                            let _ =
                                update_event_tx.send(EngineEvent::UpdateDownloaded(latest.clone()));
                        }
                        Err(e) => {
                            tracing::warn!("Update download failed: {}", e);
                        }
                    }
                }
                tracing::info!("New version available: {}", latest);
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("Update check failed: {}", e);
            }
        }
    });

    let backend = CrosstermBackend::new(stdout);
    // Start with minimal 3-line inline viewport (1 input + 1 status + 1 padding).
    // Grows upward dynamically via set_viewport_area as input lines increase.
    let mut terminal = Terminal::with_options(
        backend,
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Inline(4),
        },
    )?;

    let result = run_app(
        &mut terminal,
        &mut app,
        engine,
        tools,
        engine_event_tx,
        &mut engine_event_rx,
    )
    .await;

    // Clear the viewport before exiting so summary prints cleanly below
    terminal.clear()?;

    disable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(DisableBracketedPaste)?;

    // Move cursor below the viewport area
    let area = terminal.get_frame().area();
    crossterm::execute!(stdout, crossterm::cursor::MoveTo(0, area.bottom()))?;
    println!();

    print_exit_summary(&app);

    if let Err(ref e) = result {
        eprintln!("Yode error: {:#}", e);
    }
    result
}

/// Print session summary to stdout after exiting TUI mode.
fn print_exit_summary(app: &App) {
    if app.session.total_tokens == 0 {
        return;
    }
    let elapsed = app.session_start.elapsed();
    let mins = elapsed.as_secs() / 60;
    let secs = elapsed.as_secs() % 60;
    let duration_str = if mins > 0 {
        format!("{}m {:02}s", mins, secs)
    } else {
        format!("{}s", secs)
    };

    let cost = commands::estimate_cost(
        &app.session.model,
        app.session.input_tokens,
        app.session.output_tokens,
    );

    let session_short = &app.session.session_id[..app.session.session_id.len().min(8)];

    eprintln!();
    eprintln!("────────────────────────────────────────");
    eprintln!("Session summary");
    eprintln!(
        "  Session:       {} (resume: yode --resume {})",
        session_short, session_short
    );
    eprintln!("  Duration:      {}", duration_str);
    eprintln!(
        "  Input tokens:  {}",
        format_number(app.session.input_tokens)
    );
    eprintln!(
        "  Output tokens: {}",
        format_number(app.session.output_tokens)
    );
    eprintln!(
        "  Total tokens:  {}",
        format_number(app.session.total_tokens)
    );
    eprintln!("  Tool calls:    {}", app.session.tool_call_count);
    eprintln!("  Est. cost:     ${:.4}", cost);
    eprintln!("────────────────────────────────────────");
}

/// Format a number with comma separators (e.g. 1234 → "1,234").
fn format_number(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

// ── Event Loop ──────────────────────────────────────────────────────

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    engine: Arc<Mutex<AgentEngine>>,
    tools: Arc<ToolRegistry>,
    engine_event_tx: mpsc::UnboundedSender<EngineEvent>,
    engine_event_rx: &mut mpsc::UnboundedReceiver<EngineEvent>,
) -> Result<()> {
    loop {
        app.sync_thinking();

        // Process engine events (non-blocking)
        while let Ok(event) = engine_event_rx.try_recv() {
            handle_engine_event(app, event, &engine, &engine_event_tx);
        }
        if let Ok(engine_guard) = engine.try_lock() {
            for notification in engine_guard.drain_runtime_task_notifications() {
                app.chat_entries.push(ChatEntry::new(
                    ChatRole::System,
                    format!(
                        "[Task:{}] {}",
                        notification.severity.label(),
                        notification.message
                    ),
                ));
            }
            if app.last_task_brief_time.elapsed() >= Duration::from_secs(45) {
                let running = engine_guard
                    .runtime_tasks_snapshot()
                    .into_iter()
                    .filter(|task| matches!(task.status, yode_tools::RuntimeTaskStatus::Running))
                    .collect::<Vec<_>>();
                if !running.is_empty() {
                    let mut lines = vec!["Background tasks still running:".to_string()];
                    for task in running.into_iter().take(3) {
                        lines.push(format!(
                            "  - {} [{}] {}{}",
                            task.id,
                            task.kind,
                            task.description,
                            task
                                .last_progress
                                .as_ref()
                                .map(|progress| format!(" — {}", progress))
                                .unwrap_or_default()
                        ));
                    }
                    push_grouped_system_entry(app, "Background tasks still running", lines.join("\n"));
                    app.last_task_brief_time = Instant::now();
                }
            }
        }

        // Begin synchronized update — terminal buffers ALL output until
        // EndSynchronizedUpdate, then renders everything as a single atomic
        // frame. This eliminates flicker from insert_before's ScrollUp.
        crossterm::execute!(
            terminal.backend_mut(),
            crossterm::terminal::BeginSynchronizedUpdate
        )?;

        // 1. Flush entries to scrollback FIRST (pushes terminal up)
        flush_entries_to_scrollback(terminal, app)?;

        // 2. Resize viewport to match content height (grows up, shrinks down)
        {
            let needed = if app.wizard.is_some() {
                app.wizard.as_ref().unwrap().viewport_height() + 1 // +status
            } else if app.pending_confirmation.is_some() {
                4u16
            } else {
                let term_width = terminal.get_frame().area().width;
                let visual_lines = app.input.visual_line_count(term_width) as u16;
                let completion_lines = if app.cmd_completion.is_active() {
                    if app.cmd_completion.args_hint.is_some() {
                        1
                    } else if !app.cmd_completion.candidates.is_empty() {
                        5 // Stable height to avoid bouncing during filtering
                    } else {
                        0
                    }
                } else {
                    0
                };
                let thinking_line: u16 = if completion_lines > 0 {
                    0 // Hide status when completion is active
                } else if app.turn_status.is_visible() {
                    3 // Blank + Status + Blank
                } else {
                    0
                };
                let pending_line = app.pending_inputs.len() as u16;
                visual_lines.clamp(1, 5) + completion_lines + thinking_line + pending_line + 4
                // +separator +status_bar_separator +status_bar +blank_line
            };
            let area = terminal.get_frame().area();
            if area.height != needed {
                if needed > area.height {
                    // Growing: scroll up to make room above viewport
                    let grow_by = needed - area.height;
                    crossterm::execute!(
                        terminal.backend_mut(),
                        crossterm::terminal::ScrollUp(grow_by)
                    )?;
                    let new_y = area.y.saturating_sub(grow_by);
                    let new_area = ratatui::layout::Rect {
                        x: area.x,
                        y: new_y,
                        width: area.width,
                        height: needed,
                    };
                    terminal.viewport = ratatui::Viewport::Inline(needed);
                    terminal.set_viewport_area(new_area);
                } else {
                    // Shrinking: scroll down to pull history back, then resize
                    let shrink_by = area.height - needed;
                    let new_y = area.bottom().saturating_sub(needed);

                    // Clear the rows that were part of the TUI but are now going to be history.
                    // This avoids flickering old TUI content before history is pulled back.
                    for row in area.y..new_y {
                        crossterm::execute!(
                            terminal.backend_mut(),
                            crossterm::cursor::MoveTo(0, row),
                            crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine)
                        )?;
                    }

                    crossterm::execute!(
                        terminal.backend_mut(),
                        crossterm::terminal::ScrollDown(shrink_by)
                    )?;

                    let new_area = ratatui::layout::Rect {
                        x: area.x,
                        y: new_y,
                        width: area.width,
                        height: needed,
                    };
                    terminal.viewport = ratatui::Viewport::Inline(needed);
                    terminal.set_viewport_area(new_area);
                }
                // Force full redraw after resize
                terminal.clear()?;
            }
        }

        // 3. Draw viewport
        terminal.draw(|f| {
            ui::render(f, app);
        })?;

        // End synchronized update — terminal renders the whole frame at once
        crossterm::execute!(
            terminal.backend_mut(),
            crossterm::terminal::EndSynchronizedUpdate
        )?;

        if app.should_quit {
            break;
        }

        // Poll terminal events
        if let Some(app_event) = event::poll_event(Duration::from_millis(50))? {
            match app_event {
                AppEvent::Key(key) => {
                    // Key events are handled directly — paste detection relies on
                    // bracketed paste mode (AppEvent::Paste) and Ctrl+V/Cmd+V
                    // reading from the system clipboard via pbpaste.
                    handle_key_event(terminal, app, key, &engine, &tools, &engine_event_tx);
                }
                AppEvent::Paste(text) => {
                    let text = text.replace("\r\n", "\n").replace('\r', "\n");
                    // Wizard paste: insert text into wizard input buffer
                    if let Some(ref mut wiz) = app.wizard {
                        for c in text.chars() {
                            if c != '\n' && c != '\r' {
                                wiz.input_char(c);
                            }
                        }
                    } else if input::should_fold_paste(&text) {
                        app.input.insert_attachment(text);
                    } else {
                        for line in text.split_inclusive('\n') {
                            let clean = line.trim_end_matches('\n');
                            for c in clean.chars() {
                                app.input.insert_char(c);
                            }
                            if line.ends_with('\n') {
                                app.input.insert_newline();
                            }
                        }
                    }
                }
                AppEvent::Resize(_w, _h) => {}
                AppEvent::Tick => {
                    if app.is_thinking {
                        app.thinking.advance_spinner();
                    }
                }
            }
        }
    }

    {
        let mut engine = engine.lock().await;
        engine.finalize_session_hooks("tui_exit").await;
    }

    Ok(())
}

/// Centralized key event handler.
fn handle_key_event(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    key: crossterm::event::KeyEvent,
    engine: &Arc<Mutex<AgentEngine>>,
    tools: &Arc<ToolRegistry>,
    engine_event_tx: &mpsc::UnboundedSender<EngineEvent>,
) {
    // ── Wizard mode ────────────────────────────────────────────
    if app.wizard.is_some() {
        use crate::app::wizard::WizardStep;
        match key.code {
            KeyCode::Esc => {
                app.wizard = None;
                app.chat_entries
                    .push(ChatEntry::new(ChatRole::System, "Wizard cancelled.".into()));
            }
            KeyCode::Up => {
                if let Some(ref mut wiz) = app.wizard {
                    wiz.select_up();
                }
            }
            KeyCode::Down => {
                if let Some(ref mut wiz) = app.wizard {
                    wiz.select_down();
                }
            }
            KeyCode::Char(c) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && c == 'c' {
                    app.wizard = None;
                    app.chat_entries
                        .push(ChatEntry::new(ChatRole::System, "Wizard cancelled.".into()));
                } else if let Some(ref mut wiz) = app.wizard {
                    if matches!(wiz.current_step(), Some(WizardStep::Input { .. })) {
                        wiz.input_char(c);
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(ref mut wiz) = app.wizard {
                    if matches!(wiz.current_step(), Some(WizardStep::Input { .. })) {
                        wiz.input_backspace();
                    }
                }
            }
            KeyCode::Enter => {
                let result = app.wizard.as_mut().unwrap().submit();
                match result {
                    Ok(None) => {} // More steps
                    Ok(Some(messages)) => {
                        // Check if wizard wants to hot-reload a provider
                        let reload_name =
                            app.wizard.as_ref().and_then(|w| w.reload_provider.clone());
                        for msg in messages {
                            app.chat_entries.push(ChatEntry::new(ChatRole::System, msg));
                        }
                        if let Some(name) = reload_name {
                            reload_provider_from_config(&name, app);
                        }
                        app.wizard = None;
                    }
                    Err(e) => {
                        app.chat_entries.push(ChatEntry::new(ChatRole::Error, e));
                        app.wizard = None;
                    }
                }
            }
            _ => {}
        }
        return;
    }

    // ── History search mode ─────────────────────────────────
    if app.history.is_searching() {
        match key.code {
            KeyCode::Esc => {
                app.history.exit_search(false);
            }
            KeyCode::Enter => {
                if let Some(text) = app.history.exit_search(true) {
                    app.input.set_text(&text);
                }
            }
            KeyCode::Char(c) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && c == 'r' {
                    app.history.search_next();
                } else {
                    app.history.append_search_char(c);
                }
            }
            KeyCode::Backspace => {
                app.history.pop_search_char();
            }
            _ => {}
        }
        return;
    }

    // ── Escape: stop generation or close popup ──────────────
    if key.code == KeyCode::Esc {
        if app.is_thinking {
            app.cancel_generation();
        } else if app.cmd_completion.is_active() {
            app.cmd_completion.close();
        } else if app.file_completion.is_active() {
            app.file_completion.close();
        }
        return;
    }

    // ── Ctrl+C: stop generation, double-tap to quit ─────────
    if event::is_quit(&key) {
        if app.is_thinking {
            app.cancel_generation();
            app.last_ctrl_c = Some(Instant::now());
        } else {
            // Check for double-tap within 500ms
            let now = Instant::now();
            let is_double_tap = app
                .last_ctrl_c
                .map(|t| now.duration_since(t).as_millis() < 500)
                .unwrap_or(false);

            if is_double_tap {
                app.should_quit = true;
            } else if app.input.text().trim().is_empty() {
                // Show hint for double-tap
                app.chat_entries.push(ChatEntry::new(
                    ChatRole::System,
                    "Press Ctrl+C again to quit".to_string(),
                ));
                app.last_ctrl_c = Some(now);
            } else {
                app.input.clear();
                app.last_ctrl_c = Some(now);
            }
        }
        return;
    }

    // ── Tool confirmation (inline vertical selector) ────────
    if app.pending_confirmation.is_some() {
        match key.code {
            // Shortcut keys
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Char('1') => {
                if let Some(tx) = &app.confirm_tx {
                    let _ = tx.send(ConfirmResponse::Allow);
                }
                app.pending_confirmation = None;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('3') => {
                if let Some(tx) = &app.confirm_tx {
                    let _ = tx.send(ConfirmResponse::Deny);
                }
                app.pending_confirmation = None;
            }
            KeyCode::Char('a') | KeyCode::Char('A') | KeyCode::Char('2') => {
                if let Some(ref confirm) = app.pending_confirmation {
                    if !app.session.always_allow_tools.contains(&confirm.name) {
                        app.session.always_allow_tools.push(confirm.name.clone());
                    }
                }
                if let Some(tx) = &app.confirm_tx {
                    let _ = tx.send(ConfirmResponse::Allow);
                }
                app.pending_confirmation = None;
            }
            // Arrow navigation
            KeyCode::Up | KeyCode::Char('k') => {
                if app.confirm_selected > 0 {
                    app.confirm_selected -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if app.confirm_selected < 2 {
                    app.confirm_selected += 1;
                }
            }
            KeyCode::Enter => {
                match app.confirm_selected {
                    0 => {
                        if let Some(tx) = &app.confirm_tx {
                            let _ = tx.send(ConfirmResponse::Allow);
                        }
                    }
                    1 => {
                        if let Some(ref confirm) = app.pending_confirmation {
                            if !app.session.always_allow_tools.contains(&confirm.name) {
                                app.session.always_allow_tools.push(confirm.name.clone());
                            }
                        }
                        if let Some(tx) = &app.confirm_tx {
                            let _ = tx.send(ConfirmResponse::Allow);
                        }
                    }
                    _ => {
                        if let Some(tx) = &app.confirm_tx {
                            let _ = tx.send(ConfirmResponse::Deny);
                        }
                    }
                }
                app.pending_confirmation = None;
            }
            _ => {}
        }
        return;
    }

    // ── Main key handling ───────────────────────────────────
    match key.code {
        KeyCode::Enter => handle_enter(terminal, app, key, engine, tools, engine_event_tx),
        KeyCode::Char(c)
            if (key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::SUPER))
                && c == 'v' =>
        {
            // Ctrl+V: read from system clipboard directly (works even without BracketedPaste)
            if let Ok(output) = std::process::Command::new("pbpaste").output() {
                if output.status.success() {
                    let text = String::from_utf8_lossy(&output.stdout).to_string();
                    // Normalize line endings: \r\n → \n, bare \r → \n
                    let text = text.replace("\r\n", "\n").replace('\r', "\n");
                    if !text.is_empty() {
                        if input::should_fold_paste(&text) {
                            app.input.insert_attachment(text);
                        } else {
                            for line in text.split_inclusive('\n') {
                                let clean = line.trim_end_matches('\n');
                                for c in clean.chars() {
                                    app.input.insert_char(c);
                                }
                                if line.ends_with('\n') {
                                    app.input.insert_newline();
                                }
                            }
                        }
                    }
                }
            }
        }
        KeyCode::Char(c) => handle_char(app, key, c),
        KeyCode::Backspace => {
            app.input.backspace();
            {
                let ctx = crate::commands::context::CompletionContext {
                    provider_models: &app.provider_models,
                    all_provider_models: &app.all_provider_models,
                    provider_name: &app.provider_name,
                    tools: &app.tools,
                };
                app.cmd_completion.update(
                    &app.input.lines[0],
                    !app.input.is_multiline(),
                    &app.cmd_registry,
                    &ctx,
                );
            }
            app.file_completion.update(&app.input.text());
        }
        KeyCode::Delete => app.input.delete(),
        KeyCode::Left => app.input.move_left(),
        KeyCode::Right => app.input.move_right(),
        KeyCode::Up => handle_up(app),
        KeyCode::Down => handle_down(app),
        KeyCode::Home => app.input.move_home(),
        KeyCode::End => {
            app.input.move_end();
        }
        KeyCode::BackTab => {
            if app.file_completion.is_active() {
                app.file_completion.cycle_back();
            } else if app.cmd_completion.is_active() {
                app.cmd_completion.cycle_back();
            } else {
                app.session.permission_mode = app.session.permission_mode.next();
            }
        }
        KeyCode::Tab => handle_tab(app),
        KeyCode::PageUp => {}
        KeyCode::PageDown => {}
        _ => {}
    }
}

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
