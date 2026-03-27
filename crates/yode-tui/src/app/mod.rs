pub mod commands;
pub mod completion;
pub mod history;
pub mod input;

use std::collections::HashMap;
use std::io::{self, Write as IoWrite};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers, EnableBracketedPaste, DisableBracketedPaste};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::style::{Color, Modifier};
use ratatui::Terminal;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::error;

use yode_core::context::AgentContext;
use yode_core::db::Database;
use yode_core::engine::{AgentEngine, ConfirmResponse, EngineEvent};
use yode_core::permission::PermissionManager;
use yode_llm::provider::LlmProvider;
use yode_llm::types::Message;
use yode_tools::registry::ToolRegistry;

use crate::event::{self, AppEvent};
use crate::ui;

use self::completion::{CommandCompletion, FileCompletion};
use self::history::{BrowseResult, HistoryState};
use crate::app::input::{InputAttachment, InputState};

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
    /// When this entry was created.
    pub timestamp: Instant,
    /// If true, this entry was already printed to scrollback via streaming.
    pub already_printed: bool,
    /// Elapsed time for tool calls (set on ToolResult when matched to ToolCallStart).
    pub duration: Option<Duration>,
}

impl ChatEntry {
    pub fn new(role: ChatRole, content: String) -> Self {
        Self { role, content, timestamp: Instant::now(), already_printed: false, duration: None }
    }
}

/// Role for display purposes.
#[derive(Debug, Clone)]
pub enum ChatRole {
    User,
    Assistant,
    ToolCall { name: String },
    ToolResult { name: String, is_error: bool },
    Error,
    System,
    SubAgentCall { description: String },
    SubAgentToolCall { name: String },
    SubAgentResult,
    AskUser { id: String },
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
    pub tool_call_count: u32,
    pub permission_mode: PermissionMode,
    pub always_allow_tools: Vec<String>,
    /// True when input_tokens is estimated (provider didn't report).
    pub input_estimated: bool,
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
    /// How many lines of streaming_buf have already been printed to scrollback.
    pub streaming_printed_lines: usize,
    /// Whether we're inside a code block during streaming.
    pub streaming_in_code_block: bool,
    /// Unprinted remainder from finalized streaming: (lines, is_first_output)
    pub streaming_remainder: Option<(Vec<String>, bool)>,
    /// Whether we've printed the "Thinking..." indicator to scrollback
    pub thinking_printed: bool,

    // Engine communication
    pub pending_confirmation: Option<PendingConfirmation>,
    pub confirm_tx: Option<mpsc::UnboundedSender<ConfirmResponse>>,
    pub pending_inputs: Vec<(String, String)>,

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

    // Confirmation selection index (0=Yes, 1=Always, 2=No)
    pub confirm_selected: usize,

    // Sub-agent tracking
    pub in_sub_agent: bool,
    pub sub_agent_tool_count: usize,
}

impl App {
    pub fn new(model: String, session_id: String, working_dir: String) -> Self {
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
                tool_call_count: 0,
                permission_mode: PermissionMode::Normal,
                always_allow_tools: Vec::new(),
                input_estimated: false,
            },
            chat_entries: Vec::new(),
            printed_count: 0,
            streaming_buf: String::new(),
            streaming_printed_lines: 0,
            streaming_in_code_block: false,
            streaming_remainder: None,
            thinking_printed: false,
            pending_confirmation: None,
            confirm_tx: None,
            pending_inputs: Vec::new(),
            should_quit: false,
            is_thinking: false,
            last_ctrl_c: None,
            tool_call_starts: HashMap::new(),
            session_start: Instant::now(),
            turn_started_at: None,
            turn_tool_count: 0,
            confirm_selected: 0,
            in_sub_agent: false,
            sub_agent_tool_count: 0,
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
        let d = self.thinking.started_at.map(|s| s.elapsed()).unwrap_or_default();
        format_duration(d)
    }

    pub fn spinner_frame(&self) -> usize {
        self.thinking.spinner_frame
    }

    pub fn input_height(&self, terminal_height: u16) -> u16 {
        self.input.area_height(terminal_height)
    }
}

// ── Run TUI ─────────────────────────────────────────────────────────

/// Run the TUI application.
pub async fn run(
    provider: Arc<dyn LlmProvider>,
    tools: Arc<ToolRegistry>,
    permissions: PermissionManager,
    context: AgentContext,
    db: Database,
    restored_messages: Option<Vec<Message>>,
    skill_commands: Vec<(String, String)>,
) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnableBracketedPaste)?;
    let working_dir = context.working_dir.display().to_string();
    let is_resumed = context.is_resumed;
    let mut app = App::new(context.model.clone(), context.session_id.clone(), working_dir);
    app.cmd_completion.dynamic_commands = skill_commands;

    // Print welcome header directly to stdout before starting TUI viewport
    print_header_to_stdout(&app)?;

    // Restore messages to app state if resuming
    if let Some(ref messages) = restored_messages {
        for msg in messages {
            match msg.role {
                yode_llm::types::Role::User => {
                    if let Some(ref content) = msg.content {
                        app.chat_entries.push(ChatEntry::new(ChatRole::User, content.clone()));
                    }
                }
                yode_llm::types::Role::Assistant => {
                    if let Some(ref content) = msg.content {
                        app.chat_entries.push(ChatEntry::new(ChatRole::Assistant, content.clone()));
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
    
    // Restore messages to engine (for context)
    if let Some(ref messages) = restored_messages {
        engine_inner.restore_messages(messages.clone());
        if is_resumed {
            app.chat_entries.push(ChatEntry::new(ChatRole::System, "Session resumed.".to_string()));
        }
    }

    let engine = Arc::new(Mutex::new(engine_inner));
    let (engine_event_tx, mut engine_event_rx) = mpsc::unbounded_channel::<EngineEvent>();

    let backend = CrosstermBackend::new(stdout);
    // Fixed inline viewport: input + status bar. We allocate 5 lines to allow multiline scrolling internally.
    let mut terminal = Terminal::with_options(
        backend,
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Inline(5),
        },
    )?;

    let result = run_app(
        &mut terminal, &mut app, engine, tools,
        engine_event_tx, &mut engine_event_rx,
    ).await;

    disable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(DisableBracketedPaste)?;

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

    eprintln!("────────────────────────────────────────");
    eprintln!("Session summary");
    eprintln!("  Duration:      {}", duration_str);
    eprintln!("  Input tokens:  {}", format_number(app.session.input_tokens));
    eprintln!("  Output tokens: {}", format_number(app.session.output_tokens));
    eprintln!("  Total tokens:  {}", format_number(app.session.total_tokens));
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
            handle_engine_event(app, event);
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

        // Send queued inputs
        if !app.is_thinking && !app.pending_inputs.is_empty() {
            let (display, payload) = app.pending_inputs.remove(0);
            send_input(app, &display, &payload, &engine, &engine_event_tx);
        }

        // 2. Draw viewport AFTER (occupies exactly 4 lines at the new bottom)
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
                    handle_key_event(app, key, &engine, &tools, &engine_event_tx);
                }
                AppEvent::Paste(text) => {
                    let line_count = text.lines().count();
                    // If pasted text is more than 1 line, fold it into an attachment pill
                    if line_count > 1 {
                        // Fold into attachment
                        let id = app.input.attachments.len() + 1;
                        app.input.attachments.push(InputAttachment {
                            id,
                            name: format!("Pasted text #{}", id),
                            content: text,
                            line_count,
                        });
                    } else {
                        // Insert as raw text
                        for line in text.split_inclusive('\n') {
                            let clean = line.trim_end_matches(['\r', '\n']);
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
    Ok(())
}

/// Centralized key event handler.
fn handle_key_event(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    engine: &Arc<Mutex<AgentEngine>>,
    tools: &Arc<ToolRegistry>,
    engine_event_tx: &mpsc::UnboundedSender<EngineEvent>,
) {
    // ── History search mode ─────────────────────────────────
    if app.history.search_mode {
        match key.code {
            KeyCode::Esc => { app.history.exit_search(false); }
            KeyCode::Enter => {
                if let Some(text) = app.history.exit_search(true) {
                    app.input.set_text(&text);
                }
            }
            KeyCode::Char(c) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && c == 'r' {
                    app.history.search_next();
                } else {
                    app.history.search_query.push(c);
                    app.history.update_search();
                }
            }
            KeyCode::Backspace => {
                app.history.search_query.pop();
                app.history.update_search();
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
            let is_double_tap = app.last_ctrl_c.map(|t| now.duration_since(t).as_millis() < 500).unwrap_or(false);

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
        KeyCode::Enter => handle_enter(app, key, engine, tools, engine_event_tx),
        KeyCode::Char(c) => handle_char(app, key, c),
        KeyCode::Backspace => {
            app.input.backspace();
            app.cmd_completion.update(&app.input.lines[0], !app.input.is_multiline());
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
            app.session.permission_mode = app.session.permission_mode.next();
        }
        KeyCode::Tab => handle_tab(app),
        KeyCode::PageUp => {}
        KeyCode::PageDown => {}
        _ => {}
    }
}

fn handle_enter(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    engine: &Arc<Mutex<AgentEngine>>,
    tools: &Arc<ToolRegistry>,
    engine_event_tx: &mpsc::UnboundedSender<EngineEvent>,
) {
    // Accept completions first
    if app.cmd_completion.is_active() {
        if let Some(cmd) = app.cmd_completion.accept() {
            app.input.set_text(&cmd);
        }
        return;
    }
    if app.file_completion.is_active() {
        if let Some(path) = app.file_completion.accept() {
            let text = app.input.text();
            if let Some(at_pos) = text.rfind('@') {
                let new_text = format!("{}@{}", &text[..at_pos], path);
                app.input.set_text(&new_text);
            }
        }
        return;
    }

    // Enter to submit, Ctrl+Enter for newline
    let is_newline = key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::SUPER);

    if is_newline {
        app.input.insert_newline();
        return;
    }

    // Submit input
    let (display, payload, raw_typed) = app.input.take();
    if payload.trim().is_empty() {
        return;
    }

    app.history.push(raw_typed.clone());
    app.cmd_completion.close();
    app.file_completion.close();

    // Shell command
    if app.handle_shell_command(&raw_typed) {
        return;
    }

    // Slash command
    if app.handle_slash_command(&raw_typed, tools, engine) {
        return;
    }

    // Process @file references
    let processed_payload = app.process_file_references(&payload);
    let processed_display = app.process_file_references(&display);

    if app.is_thinking {
        app.chat_entries.push(ChatEntry::new(ChatRole::User, processed_display.clone()));
        app.pending_inputs.push((processed_display, processed_payload));
    } else if app.session.permission_mode == PermissionMode::Plan {
        app.chat_entries.push(ChatEntry::new(ChatRole::User, processed_display.clone()));
        app.chat_entries.push(ChatEntry::new(
            ChatRole::System,
            "[Plan mode] Input recorded. Switch to Normal or Auto-Accept to execute.".to_string(),
        ));
    } else {
        send_input(app, &processed_display, &processed_payload, engine, engine_event_tx);
    }
}

fn handle_char(app: &mut App, key: crossterm::event::KeyEvent, c: char) {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match c {
            'a' => app.input.move_home(),
            'e' => app.input.move_end(),
            'u' => app.input.clear(),
            'k' => app.input.kill_to_end(),
            'w' => app.input.delete_word_back(),
            'l' => {
                app.chat_entries.clear();
                app.printed_count = 0;
            }
            'j' => app.input.insert_newline(),
            'r' => app.history.enter_search(),
            'p' => browse_history_prev(app),
            'n' => browse_history_next(app),
            _ => {}
        }
    } else {
        app.input.insert_char(c);
        app.cmd_completion.update(&app.input.lines[0], !app.input.is_multiline());
        if c == '@' || app.file_completion.is_active() {
            app.file_completion.update(&app.input.text());
        }
    }
}

fn handle_up(app: &mut App) {
    if app.input.is_multiline() {
        app.input.move_up();
    } else {
        browse_history_prev(app);
    }
}

fn handle_down(app: &mut App) {
    if app.input.is_multiline() {
        app.input.move_down();
    } else {
        browse_history_next(app);
    }
}



/// Browse to previous history entry (Ctrl+P or Up with text).
fn browse_history_prev(app: &mut App) {
    if !app.history.is_browsing() {
        app.history.start_browse(app.input.lines.clone());
    }
    if let Some(entry) = app.history.current_browse_entry() {
        app.input.set_text(entry);
    }
    if let Some(text) = app.history.browse_prev() {
        app.input.set_text(text);
    }
}

/// Browse to next history entry (Ctrl+N or Down with text).
fn browse_history_next(app: &mut App) {
    match app.history.browse_next() {
        BrowseResult::Entry(text) => app.input.set_text(&text),
        BrowseResult::Restore(lines) => {
            app.input.lines = lines;
            app.input.cursor_line = 0;
            app.input.cursor_col = app.input.lines[0].chars().count();
        }
        BrowseResult::None => {}
    }
}

fn handle_tab(app: &mut App) {
    if app.file_completion.is_active() {
        if app.file_completion.candidates.len() == 1 {
            if let Some(path) = app.file_completion.accept() {
                let text = app.input.text();
                if let Some(at_pos) = text.rfind('@') {
                    let new_text = format!("{}@{}", &text[..at_pos], path);
                    app.input.set_text(&new_text);
                }
            }
        } else {
            app.file_completion.cycle();
        }
    } else if app.cmd_completion.is_active() {
        if app.cmd_completion.candidates.len() == 1 {
            if let Some(cmd) = app.cmd_completion.accept() {
                app.input.set_text(&cmd);
            }
        } else {
            app.cmd_completion.cycle();
        }
    } else {
        app.cmd_completion.update(&app.input.lines[0], !app.input.is_multiline());
        if app.cmd_completion.candidates.len() == 1 {
            if let Some(cmd) = app.cmd_completion.accept() {
                app.input.set_text(&cmd);
            }
        }
    }
}

// ── Engine communication ────────────────────────────────────────────

fn send_input(
    app: &mut App,
    display: &str,
    payload: &str,
    engine: &Arc<Mutex<AgentEngine>>,
    engine_event_tx: &mpsc::UnboundedSender<EngineEvent>,
) {
    // Add user message (if not already queued)
    let already_added = app.chat_entries.last()
        .map_or(false, |e| matches!(e.role, ChatRole::User) && e.content == display);
    if !already_added {
        app.chat_entries.push(ChatEntry::new(ChatRole::User, display.to_string()));
    }

    let cancel_token = CancellationToken::new();
    app.thinking.start(cancel_token.clone());
    app.turn_started_at = Some(Instant::now());
    app.turn_tool_count = 0;
    app.sync_thinking();

    let (confirm_tx, confirm_rx) = mpsc::unbounded_channel();
    app.confirm_tx = Some(confirm_tx);

    let engine = engine.clone();
    let event_tx = engine_event_tx.clone();
    let input_owned = payload.to_string();
    tokio::spawn(async move {
        let mut engine = engine.lock().await;
        let result = engine
            .run_turn_streaming(&input_owned, event_tx.clone(), confirm_rx, Some(cancel_token))
            .await;
        if let Err(e) = result {
            error!("Engine turn error: {}", e);
            let _ = event_tx.send(EngineEvent::Error(format!("Engine error: {}", e)));
            let _ = event_tx.send(EngineEvent::Done);
        }
    });
}

fn handle_engine_event(app: &mut App, event: EngineEvent) {
    match event {
        EngineEvent::Thinking => {
            app.thinking.active = true;
            app.sync_thinking();
        }
        EngineEvent::TextDelta(delta) => {
            // Append to streaming buffer instead of creating/appending ChatEntry.
            // flush_entries_to_scrollback will print new lines.
            app.streaming_buf.push_str(&delta);
        }
        EngineEvent::TextComplete(text) => {
            // If we already have streaming content, it contains the full text.
            // If not, set it now.
            if app.streaming_buf.is_empty() {
                app.streaming_buf = text;
            }
        }
        EngineEvent::ToolCallStart { id, name, arguments } => {
            // Finalize any streaming buffer into a ChatEntry first.
            finalize_streaming(app);
            app.turn_tool_count += 1;

            // If we're inside a sub-agent, create SubAgentToolCall instead
            if app.in_sub_agent {
                app.sub_agent_tool_count += 1;
                app.session.tool_call_count += 1;
                app.tool_call_starts.insert(id, Instant::now());
                app.chat_entries.push(ChatEntry::new(
                    ChatRole::SubAgentToolCall { name },
                    arguments,
                ));
                return;
            }

            // Check if this is a re-send with full args (engine sends again
            // for auto-allowed tools after args are accumulated in streaming).
            let existing = app.chat_entries.iter_mut().rev().take(5).find(|e| {
                matches!(&e.role, ChatRole::ToolCall { name: n } if n == &name)
                    && e.content.is_empty()
            });
            if let Some(entry) = existing {
                // Update existing entry with full arguments
                entry.content = arguments;
            } else if !app.chat_entries.iter().rev().take(3).any(|e| {
                matches!(&e.role, ChatRole::ToolCall { name: n } if n == &name)
                    && !e.content.is_empty()
            }) {
                // New tool call — create entry
                app.session.tool_call_count += 1;
                app.tool_call_starts.insert(id, Instant::now());
                app.chat_entries.push(ChatEntry::new(
                    ChatRole::ToolCall { name },
                    arguments,
                ));
            }
        }
        EngineEvent::ToolConfirmRequired { id, name, arguments } => {
            // Update the existing ToolCall entry with full arguments (streaming
            // initially sends empty args), or create if somehow missing.
            let existing = app.chat_entries.iter_mut().rev().take(5).find(|e| {
                matches!(&e.role, ChatRole::ToolCall { name: n } if n == &name)
            });
            if let Some(entry) = existing {
                if entry.content.is_empty() {
                    entry.content = arguments.clone();
                }
            } else {
                app.chat_entries.push(ChatEntry::new(
                    ChatRole::ToolCall { name: name.clone() },
                    arguments.clone(),
                ));
            }

            if app.session.permission_mode == PermissionMode::AutoAccept
                || app.session.always_allow_tools.contains(&name)
            {
                if let Some(tx) = &app.confirm_tx {
                    let _ = tx.send(ConfirmResponse::Allow);
                }
            } else {
                app.pending_confirmation = Some(PendingConfirmation { id, name, arguments });
                app.confirm_selected = 0;
            }
        }
        EngineEvent::ToolResult { id, name, result } => {
            let duration = app.tool_call_starts.remove(&id).map(|start| start.elapsed());
            let mut entry = ChatEntry::new(
                ChatRole::ToolResult { name, is_error: result.is_error },
                result.content,
            );
            entry.duration = duration;
            app.chat_entries.push(entry);
        }
        EngineEvent::TurnComplete(response) => {
            finalize_streaming(app);

            // Determine input tokens: use reported value, infer from total, or estimate
            let prompt = response.usage.prompt_tokens;
            let completion = response.usage.completion_tokens;
            let total = response.usage.total_tokens;

            if prompt > 0 {
                // Provider reports input tokens — accumulate
                app.session.input_tokens += prompt;
            } else if total > completion {
                // Infer from total - completion
                app.session.input_tokens += total - completion;
            } else {
                // Provider doesn't report input tokens at all — estimate from content.
                // All chat content is sent as context each turn; ~3 chars per token.
                let chars: usize = app.chat_entries.iter().map(|e| e.content.len()).sum();
                app.session.input_tokens = (chars as u32) / 3;
                app.session.input_estimated = true;
            }

            app.session.output_tokens += completion;
            app.session.total_tokens = app.session.input_tokens + app.session.output_tokens;
            app.thinking.stop();
            app.thinking_printed = false;
            app.sync_thinking();
        }
        EngineEvent::Error(e) => {
            finalize_streaming(app);
            app.thinking_printed = false;
            app.chat_entries.push(ChatEntry::new(ChatRole::Error, e));
        }
        EngineEvent::AskUser { id: _, question } => {
            finalize_streaming(app);
            app.chat_entries.push(ChatEntry::new(
                ChatRole::System,
                format!("❓ {}", question),
            ));
            // The ask_user tool will block waiting for a response via the
            // dedicated ask_user channel. For now, treat like a system message.
            // Full interactive ask_user support will be added with TUI channel wiring.
        }
        EngineEvent::Done => {
            finalize_streaming(app);

            // Print turn completion summary with timing
            if let Some(started) = app.turn_started_at.take() {
                let elapsed = started.elapsed();
                let tools = app.turn_tool_count;
                let summary = if tools > 0 {
                    format!("⚡ Done · {} · {} tool calls", format_duration(elapsed), tools)
                } else {
                    format!("⚡ Done · {}", format_duration(elapsed))
                };
                app.chat_entries.push(ChatEntry::new(ChatRole::System, summary));
            }

            app.thinking.stop();
            app.thinking_printed = false;
            app.sync_thinking();
            app.tool_call_starts.clear();
        }
        EngineEvent::SubAgentStart { description } => {
            finalize_streaming(app);
            app.in_sub_agent = true;
            app.sub_agent_tool_count = 0;
            app.chat_entries.push(ChatEntry::new(
                ChatRole::SubAgentCall { description },
                String::new(),
            ));
        }
        EngineEvent::SubAgentComplete { result } => {
            app.in_sub_agent = false;
            let mut entry = ChatEntry::new(
                ChatRole::SubAgentResult,
                result,
            );
            // Calculate duration from SubAgentCall timestamp
            if let Some(call_entry) = app.chat_entries.iter().rev().find(|e| {
                matches!(&e.role, ChatRole::SubAgentCall { .. })
            }) {
                entry.duration = Some(call_entry.timestamp.elapsed());
            }
            app.chat_entries.push(entry);
        }
        EngineEvent::PlanModeEntered => {
            app.chat_entries.push(ChatEntry::new(
                ChatRole::System,
                "📋 Entered plan mode (read-only tools only)".to_string(),
            ));
        }
        EngineEvent::PlanApprovalRequired { plan_content } => {
            let preview = if plan_content.len() > 500 {
                format!("{}...", &plan_content[..500])
            } else {
                plan_content
            };
            app.chat_entries.push(ChatEntry::new(
                ChatRole::System,
                format!("📋 Plan ready for approval:\n{}", preview),
            ));
        }
        EngineEvent::PlanModeExited => {
            app.chat_entries.push(ChatEntry::new(
                ChatRole::System,
                "📋 Exited plan mode".to_string(),
            ));
        }
        EngineEvent::ContextCompressed { removed } => {
            app.chat_entries.push(ChatEntry::new(
                ChatRole::System,
                format!("Context compressed: removed {} messages to fit window.", removed),
            ));
        }
    }
}

/// Move streaming_buf content into a ChatEntry and reset streaming state.
/// Save any unprinted remainder for flush to output.
fn finalize_streaming(app: &mut App) {
    if !app.streaming_buf.is_empty() {
        let content = std::mem::take(&mut app.streaming_buf);
        let all_lines: Vec<&str> = content.lines().collect();
        let printed = app.streaming_printed_lines;

        // Save unprinted tail lines for flush
        if printed < all_lines.len() {
            let remainder: Vec<String> = all_lines[printed..].iter().map(|s| s.to_string()).collect();
            app.streaming_remainder = Some((remainder, printed == 0));
        }

        let mut entry = ChatEntry::new(ChatRole::Assistant, content.clone());
        entry.already_printed = true; // always true — remainder handled separately
        // Don't push empty/whitespace-only assistant entries
        if !content.trim().is_empty() {
            app.chat_entries.push(entry);
        }
        app.streaming_printed_lines = 0;
        app.streaming_in_code_block = false;
    }
}

// ── Scrollback printing ─────────────────────────────────────────────

/// Print lines to terminal scrollback.
/// Uses insert_before to scroll viewport, then writes ANSI-colored text
/// directly to the backend, completely bypassing ratatui's Buffer/cell system.
fn raw_print_lines(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    lines: &[(String, Option<crossterm::style::Color>, bool)],
) -> Result<()> {
    if lines.is_empty() { return Ok(()); }

    // Calculate actual terminal rows needed, accounting for CJK/wide-char wrapping.
    // A logical line wider than the terminal wraps to multiple rows.
    let term_width = crossterm::terminal::size()?.0 as usize;
    let actual_rows: usize = lines.iter().map(|(text, color, _)| {
        // Strip ANSI codes for accurate width when line has embedded colors
        let visible = if color.is_none() && text.contains('\x1b') {
            unicode_width::UnicodeWidthStr::width(strip_ansi(text).as_str())
        } else {
            unicode_width::UnicodeWidthStr::width(text.as_str())
        };
        if visible == 0 || term_width == 0 { 1 } else { visible.div_ceil(term_width).max(1) }
    }).sum();

    // Step 1: Create blank space above viewport
    terminal.insert_before(actual_rows as u16, |_buf| {})?;

    // Step 2: Write directly to the underlying stdout via backend.
    let backend = terminal.backend_mut();

    // Move cursor up from viewport start to the first blank line
    crossterm::queue!(backend,
        crossterm::cursor::MoveUp(actual_rows as u16),
    )?;

    for (text, color, bold) in lines {
        crossterm::queue!(backend, crossterm::cursor::MoveToColumn(0))?;
        crossterm::queue!(backend, crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine))?;
        if *bold {
            crossterm::queue!(backend, crossterm::style::SetAttribute(crossterm::style::Attribute::Bold))?;
        }
        if let Some(c) = color {
            crossterm::queue!(backend, crossterm::style::SetForegroundColor(*c))?;
        }
        // Write the ENTIRE text line as a single Print — no cell-by-cell rendering
        crossterm::queue!(backend, crossterm::style::Print(text))?;
        crossterm::queue!(backend, crossterm::style::ResetColor)?;
        if *bold {
            crossterm::queue!(backend, crossterm::style::SetAttribute(crossterm::style::Attribute::NoBold))?;
        }
        crossterm::queue!(backend, crossterm::cursor::MoveToNextLine(1))?;
    }

    backend.flush()?;
    Ok(())
}

/// Print the welcome header into terminal stdout before starting TUI.
fn print_header_to_stdout(app: &App) -> Result<()> {
    let width = crossterm::terminal::size()?.0 as usize;
    let header_lines = ui::chat::render_header(app, width);
    
    let mut stdout = io::stdout();
    for line in header_lines {
        // Convert ratatui Line to colored strings for raw stdout
        for span in line.spans {
            if let Some(color) = span.style.fg {
                let c = match color {
                    Color::Rgb(r, g, b) => crossterm::style::Color::Rgb { r, g, b },
                    _ => crossterm::style::Color::White,
                };
                stdout.execute(crossterm::style::SetForegroundColor(c))?;
            }
            if span.style.add_modifier.contains(Modifier::BOLD) {
                stdout.execute(crossterm::style::SetAttribute(crossterm::style::Attribute::Bold))?;
            }
            stdout.execute(crossterm::style::Print(&span.content))?;
            stdout.execute(crossterm::style::SetAttribute(crossterm::style::Attribute::Reset))?;
        }
        stdout.execute(crossterm::style::Print("\r\n"))?;
    }
    stdout.flush()?;
    Ok(())
}

fn print_entries_to_stdout(app: &mut App) -> Result<()> {
    if app.chat_entries.is_empty() { return Ok(()); }
    
    let mut stdout = io::stdout();
    for i in 0..app.chat_entries.len() {
        let entry = &app.chat_entries[i];
        let text_lines = format_entry_as_strings(entry, &app.chat_entries, i);
        
        if i > 0 && matches!(entry.role, ChatRole::User) {
            stdout.execute(crossterm::style::Print("\r\n"))?;
        }
        
        for (text, style) in text_lines {
            if let Some(color) = style.fg {
                let c = match color {
                    Color::Rgb(r, g, b) => crossterm::style::Color::Rgb { r, g, b },
                    _ => crossterm::style::Color::White,
                };
                stdout.execute(crossterm::style::SetForegroundColor(c))?;
            }
            if style.add_modifier.contains(Modifier::BOLD) {
                stdout.execute(crossterm::style::SetAttribute(crossterm::style::Attribute::Bold))?;
            }
            stdout.execute(crossterm::style::Print(text))?;
            stdout.execute(crossterm::style::SetAttribute(crossterm::style::Attribute::Reset))?;
            stdout.execute(crossterm::style::Print("\r\n"))?;
        }
    }
    app.printed_count = app.chat_entries.len();
    stdout.flush()?;
    Ok(())
}

/// Flush new chat entries and streaming lines to the terminal scrollback.
/// Format a duration as human-readable string.
fn format_duration(d: Duration) -> String {
    let total_secs = d.as_secs();
    if total_secs >= 60 {
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        if secs == 0 { format!("{}m", mins) } else { format!("{}m {}s", mins, secs) }
    } else if total_secs > 0 {
        format!("{:.1}s", d.as_secs_f64())
    } else {
        format!("{}ms", d.as_millis())
    }
}

/// Determine color and bold for a markdown-processed line based on its prefix.
fn md_line_color(line: &str) -> (crossterm::style::Color, bool) {
    if line.starts_with("━━ ") || line.starts_with("━━━") {
        (crossterm::style::Color::Rgb { r: 230, g: 190, b: 60 }, true)
    } else if line.starts_with("▸ ") {
        (crossterm::style::Color::Rgb { r: 100, g: 140, b: 255 }, true)
    } else if line.starts_with("  ▹ ") {
        (crossterm::style::Color::Rgb { r: 130, g: 160, b: 255 }, false)
    } else if line.starts_with("    ") && !line.trim().is_empty() {
        // Code block lines (4-space indent) — syntax highlight color
        (crossterm::style::Color::Rgb { r: 180, g: 220, b: 170 }, false)
    } else if line.starts_with("▎ ") {
        (crossterm::style::Color::Rgb { r: 160, g: 150, b: 120 }, false)
    } else if line.starts_with("────") {
        (crossterm::style::Color::Rgb { r: 70, g: 70, b: 80 }, false)
    } else if line.starts_with("── ") || line.starts_with("───") {
        (crossterm::style::Color::Rgb { r: 100, g: 200, b: 220 }, true)
    } else if line.contains('│') {
        (crossterm::style::Color::Rgb { r: 200, g: 200, b: 215 }, false)
    } else {
        (crossterm::style::Color::Rgb { r: 220, g: 220, b: 230 }, false)
    }
}

fn flush_entries_to_scrollback(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    // Collect ALL output into a single buffer to minimize insert_before calls
    let mut all_output: Vec<(String, Option<crossterm::style::Color>, bool)> = Vec::new();

    // 1. Print streaming buffer — ONLY complete lines (ending with \n).
    //    Partial last line stays in buffer until more text or finalization.
    if !app.streaming_buf.is_empty() {
        // Count complete lines (each \n = one complete line)
        let complete_count = app.streaming_buf.matches('\n').count();
        if complete_count > app.streaming_printed_lines {
            // Get all lines, but only print up to complete_count
            let all_lines: Vec<&str> = app.streaming_buf.lines().collect();
            let to_print = &all_lines[app.streaming_printed_lines..complete_count.min(all_lines.len())];

            let needs_spacer = app.streaming_printed_lines == 0;
            let mut first_printed = app.streaming_printed_lines > 0;
            for raw_text in to_print.iter() {
                // Skip leading whitespace-only lines (before first real content)
                if !first_printed && raw_text.trim().is_empty() {
                    continue;
                }
                let is_first = !first_printed;
                // Add blank line before first AI response line (visual separation from user input)
                if is_first && needs_spacer {
                    all_output.push((String::new(), None, false));
                }
                // Process markdown on each complete line
                let text = process_md_line(raw_text, &mut app.streaming_in_code_block);
                let prefix = if is_first { "⏺ " } else { "  " };
                if is_first {
                    let color = crossterm::style::Color::Rgb { r: 140, g: 120, b: 255 };
                    all_output.push((format!("{}{}", prefix, text), Some(color), false));
                    first_printed = true;
                } else if is_code_block_line(&text) {
                    // Code line — apply syntax highlighting, no line-level color
                    let highlighted = highlight_code_line(&text);
                    all_output.push((format!("{}{}", prefix, highlighted), None, false));
                } else {
                    let (color, bold) = md_line_color(&text);
                    all_output.push((format!("{}{}", prefix, text), Some(color), bold));
                }
            }
            app.streaming_printed_lines = complete_count;
        }
    }

    // 1b. Print any streaming remainder (last line that didn't end with \n)
    if let Some((remainder, is_first)) = app.streaming_remainder.take() {
        let has_content = remainder.iter().any(|l| !l.trim().is_empty());
        if has_content {
            let accent = crossterm::style::Color::Rgb { r: 140, g: 120, b: 255 };
            let mut first_done = !is_first;
            for line in remainder.iter() {
                // Skip leading empty lines only
                if !first_done && line.trim().is_empty() {
                    continue;
                }
                let text = process_md_line(line, &mut app.streaming_in_code_block);
                if !first_done {
                    all_output.push((String::new(), None, false));
                    all_output.push((format!("⏺ {}", text), Some(accent), false));
                    first_done = true;
                } else if is_code_block_line(&text) {
                    let highlighted = highlight_code_line(&text);
                    all_output.push((format!("  {}", highlighted), None, false));
                } else {
                    let (color, bold) = md_line_color(&text);
                    all_output.push((format!("  {}", text), Some(color), bold));
                }
            }
        }
    }

    // 2. Print completed entries
    while app.printed_count < app.chat_entries.len() {
        let entry = &app.chat_entries[app.printed_count];

        if entry.already_printed {
            app.printed_count += 1;
            continue;
        }

        // Defer ToolCall until its ToolResult is available, so the inline
        // result + timing display works correctly in scrollback.
        if let ChatRole::ToolCall { ref name } = entry.role {
            let tool_name = name.clone();
            let has_result = app.chat_entries[app.printed_count + 1..].iter().any(|e| {
                matches!(&e.role, ChatRole::ToolResult { name: ref n, .. } if *n == tool_name)
            });
            if !has_result {
                break; // Wait for result before printing
            }
        }

        // Defer SubAgentCall until SubAgentResult arrives, so the nested
        // block renders as a complete tree with timing.
        if matches!(entry.role, ChatRole::SubAgentCall { .. }) {
            let has_result = app.chat_entries[app.printed_count + 1..].iter().any(|e| {
                matches!(&e.role, ChatRole::SubAgentResult)
            });
            if !has_result {
                break; // Wait for sub-agent to complete
            }
        }

        // Skip SubAgentToolCall and SubAgentResult — rendered by SubAgentCall
        if matches!(entry.role, ChatRole::SubAgentToolCall { .. } | ChatRole::SubAgentResult) {
            app.printed_count += 1;
            continue;
        }

        let text_lines = format_entry_as_strings(entry, &app.chat_entries, app.printed_count);
        let needs_spacer = matches!(entry.role, ChatRole::User) && app.printed_count > 0;

        if needs_spacer {
            all_output.push((String::new(), None, false));
        }
        for (text, style) in &text_lines {
            let color = style.fg.map(|c| match c {
                Color::Rgb(r, g, b) => crossterm::style::Color::Rgb { r, g, b },
                _ => crossterm::style::Color::White,
            });
            let bold = style.add_modifier.contains(Modifier::BOLD);
            all_output.push((text.clone(), color, bold));
        }

        app.printed_count += 1;
    }

    // Single raw_print_lines call for all accumulated output
    if !all_output.is_empty() {
        raw_print_lines(terminal, &all_output)?;
    }

    Ok(())
}

/// Format a ChatEntry as plain (String, Style) pairs.
fn format_entry_as_strings(
    entry: &ChatEntry,
    all_entries: &[ChatEntry],
    index: usize,
) -> Vec<(String, ratatui::style::Style)> {
    let mut result: Vec<(String, ratatui::style::Style)> = Vec::new();
    let white = ratatui::style::Style::default().fg(Color::Rgb(220, 220, 230));
    let dim = ratatui::style::Style::default().fg(Color::Rgb(90, 90, 100));
    let accent = ratatui::style::Style::default().fg(Color::Rgb(140, 120, 255));
    let bold_white = white.add_modifier(Modifier::BOLD);
    let red = ratatui::style::Style::default().fg(Color::Rgb(240, 80, 80));

    match &entry.role {
        ChatRole::User => {
            result.push((format!("> {}", entry.content), bold_white));
        }
        ChatRole::Assistant => {
            let processed = markdown_to_plain(&entry.content);
            // Skip empty/whitespace assistant entries (LLM sometimes sends blank text between tool calls)
            if processed.trim().is_empty() {
                return result;
            }
            let mut first = true;
            for line in processed.lines() {
                if line.trim().is_empty() {
                    result.push((String::new(), dim));
                    continue;
                }
                if first {
                    result.push((format!("⏺ {}", line), accent));
                    first = false;
                } else if is_code_block_line(&line) {
                    // Code line — embed ANSI highlighting, no ratatui fg color
                    let highlighted = highlight_code_line(&line);
                    result.push((format!("  {}", highlighted), ratatui::style::Style::default()));
                } else {
                    let (ct_color, bold) = md_line_color(&line);
                    let color = if let crossterm::style::Color::Rgb { r, g, b } = ct_color {
                        Color::Rgb(r, g, b)
                    } else {
                        Color::Rgb(220, 220, 230)
                    };
                    let mut style = ratatui::style::Style::default().fg(color);
                    if bold {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    result.push((format!("  {}", line), style));
                }
            }
        }
        ChatRole::ToolCall { name } => {
            let args: serde_json::Value = serde_json::from_str(&entry.content).unwrap_or_default();

            let tool_result = all_entries[index + 1..].iter().find(|e| {
                matches!(&e.role, ChatRole::ToolResult { name: n, .. } if n == name)
            });

            // Format timing suffix from the matching ToolResult's duration
            let timing = tool_result
                .and_then(|r| r.duration)
                .map(|d| {
                    if d.as_secs() >= 1 {
                        format!(" ── {:.1}s", d.as_secs_f64())
                    } else {
                        format!(" ── {}ms", d.as_millis())
                    }
                })
                .unwrap_or_default();

            let green = ratatui::style::Style::default().fg(Color::Rgb(80, 200, 120));
            let red_dim = ratatui::style::Style::default().fg(Color::Rgb(200, 80, 80));

            // Special display for edit_file: show Claude-style diff
            if name == "edit_file" {
                let file_path = args["file_path"].as_str().unwrap_or("???");
                // Shorten path: show relative if under cwd
                let display_path = file_path.strip_prefix(&format!("{}/",
                    std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default()
                )).unwrap_or(file_path);

                let old_str = args["old_string"].as_str().unwrap_or("");
                let new_str = args["new_string"].as_str().unwrap_or("");
                let old_lines: Vec<&str> = old_str.lines().collect();
                let new_lines: Vec<&str> = new_str.lines().collect();
                let added = new_lines.len();
                let removed = old_lines.len();

                let summary = if added > 0 && removed > 0 {
                    format!("Added {} lines, removed {} lines", added, removed)
                } else if added > 0 {
                    format!("Added {} lines", added)
                } else {
                    format!("Removed {} lines", removed)
                };

                result.push((format!("⏺ Update({}){}", display_path, timing), accent));
                result.push((format!("  ⎿  {}", summary), dim));

                // Show diff: removed lines with -, added lines with +
                let max_diff = 6;
                let mut shown = 0;
                let total = old_lines.len() + new_lines.len();
                for line in &old_lines {
                    if shown >= max_diff {
                        result.push((format!("     … +{} lines (ctrl+o to expand)", total - shown), dim));
                        break;
                    }
                    result.push((format!("     - {}", line), red_dim));
                    shown += 1;
                }
                if shown < max_diff {
                    for line in &new_lines {
                        if shown >= max_diff {
                            result.push((format!("     … +{} lines (ctrl+o to expand)", total - shown), dim));
                            break;
                        }
                        result.push((format!("     + {}", line), green));
                        shown += 1;
                    }
                }
            } else if name == "read_file" {
                // Read: just show the path, no content
                let file_path = args["file_path"].as_str().unwrap_or("???");
                let display_path = file_path.strip_prefix(&format!("{}/",
                    std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default()
                )).unwrap_or(file_path);
                result.push((format!("⏺ Read({}){}", display_path, timing), accent));
            } else if name == "write_file" {
                // Write: show path + first few lines of content
                let file_path = args["file_path"].as_str().unwrap_or("???");
                let display_path = file_path.strip_prefix(&format!("{}/",
                    std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default()
                )).unwrap_or(file_path);
                let content = args["content"].as_str().unwrap_or("");
                let total_lines = content.lines().count();
                result.push((format!("⏺ Write({}){}", display_path, timing), accent));
                result.push((format!("  ⎿  {} lines written", total_lines), dim));
                let max_preview = 3;
                for (i, line) in content.lines().enumerate() {
                    if i >= max_preview {
                        result.push((format!("     … +{} lines (ctrl+o to expand)", total_lines - max_preview), dim));
                        break;
                    }
                    let green = ratatui::style::Style::default().fg(Color::Rgb(80, 200, 120));
                    result.push((format!("     + {}", line), green));
                }
            } else {
                let summary = tool_summary_str(name, &args);
                result.push((format!("⏺ {}({}){}", capitalize(name), summary, timing), accent));

                if let Some(res) = tool_result {
                    let max_lines = 3;
                    let max_line_chars = crossterm::terminal::size()
                        .map(|(w, _)| (w as usize).saturating_sub(10))
                        .unwrap_or(120);
                    for (i, line) in res.content.lines().enumerate() {
                        if i >= max_lines {
                            result.push((format!("     … +{} lines (ctrl+o to expand)", res.content.lines().count() - max_lines), dim));
                            break;
                        }
                        let prefix = if i == 0 { "  ⎿  " } else { "     " };
                        let style = if matches!(res.role, ChatRole::ToolResult { is_error, .. } if is_error) { red } else { dim };
                        let display = truncate_line(line, max_line_chars);
                        result.push((format!("{}{}", prefix, display), style));
                    }
                }
            }
        }
        ChatRole::ToolResult { .. } => {
            let has_preceding = index > 0 && all_entries[..index].iter().rev().any(|e| {
                matches!(&e.role, ChatRole::ToolCall { name: n } if {
                    if let ChatRole::ToolResult { name: rn, .. } = &entry.role { n == rn } else { false }
                })
            });
            if !has_preceding {
                result.push((format!("  ⎿ {}", entry.content.lines().next().unwrap_or("")), dim));
            }
        }
        ChatRole::Error => {
            result.push((format!("! {}", entry.content), red));
        }
        ChatRole::System => {
            for line in entry.content.lines() {
                result.push((format!("  {}", line), dim));
            }
        }
        ChatRole::SubAgentCall { description } => {
            // Look forward for SubAgentToolCall entries and SubAgentResult
            let mut sub_tools: Vec<String> = Vec::new();
            let mut agent_duration: Option<Duration> = None;
            for e in &all_entries[index + 1..] {
                match &e.role {
                    ChatRole::SubAgentToolCall { name } => {
                        sub_tools.push(name.clone());
                    }
                    ChatRole::SubAgentResult => {
                        agent_duration = e.duration;
                        break;
                    }
                    _ => break,
                }
            }

            // Extract agent type from description (e.g. "Explore" from "Analyze transapi project")
            // Try to detect agent type from the description or tool name pattern
            let agent_type = if description.to_lowercase().contains("explore") {
                "Explore"
            } else if description.to_lowercase().contains("plan") {
                "Plan"
            } else {
                "Agent"
            };

            let timing = agent_duration
                .map(|d| format!(" ── {}", format_duration(d)))
                .unwrap_or_default();

            result.push((format!("⏺ {}({}){}", agent_type, description, timing), accent));

            // Show first 3 sub-tools, then truncate
            let max_show = 3;
            let total = sub_tools.len();
            for (i, tool_name) in sub_tools.iter().enumerate() {
                if i >= max_show {
                    result.push((format!("     … +{} more tool uses (ctrl+o to expand)", total - max_show), dim));
                    break;
                }
                let prefix = if i == 0 { "  ⎿  " } else { "     " };
                result.push((format!("{}{}(…)", prefix, capitalize(tool_name)), dim));
            }
            if total == 0 {
                result.push(("  ⎿  (no tool calls)".to_string(), dim));
            }
        }
        ChatRole::SubAgentToolCall { .. } => {
            // Rendered by SubAgentCall — return empty
        }
        ChatRole::SubAgentResult => {
            // Timing merged into SubAgentCall — return empty
        }
        ChatRole::AskUser { .. } => {
            // Rendered as system-like prefix in handle_engine_event
        }
    }
    result
}

fn tool_summary_str(name: &str, args: &serde_json::Value) -> String {
    match name {
        "bash" => args["command"].as_str().unwrap_or("???").to_string(),
        "write_file" | "read_file" => args["file_path"].as_str().unwrap_or("???").to_string(),
        "edit_file" => args["file_path"].as_str().unwrap_or("???").to_string(),
        "glob" => args["pattern"].as_str().unwrap_or("???").to_string(),
        "grep" => args["pattern"].as_str().unwrap_or("???").to_string(),
        "agent" => args["description"].as_str().unwrap_or("???").to_string(),
        "memory" => {
            let action = args["action"].as_str().unwrap_or("???");
            let mem_name = args["name"].as_str().unwrap_or("");
            if mem_name.is_empty() { action.to_string() } else { format!("{} {}", action, mem_name) }
        }
        "cron" => args["action"].as_str().unwrap_or("???").to_string(),
        "lsp" => {
            let op = args["operation"].as_str().unwrap_or("???");
            let file = args["filePath"].as_str().unwrap_or("");
            if file.is_empty() { op.to_string() } else { format!("{} {}", op, file) }
        }
        "enter_worktree" => args["name"].as_str().unwrap_or("").to_string(),
        "notebook_edit" => args["notebook_path"].as_str().unwrap_or("???").to_string(),
        _ => {
            if let Some(obj) = args.as_object() {
                // Try common argument keys
                for key in &["command", "path", "file_path", "relative_path", "query", "pattern", "url", "name"] {
                    if let Some(val) = obj.get(*key).and_then(|v| v.as_str()) {
                        return val.to_string();
                    }
                }
                // Fallback: show first string value
                for val in obj.values() {
                    if let Some(s) = val.as_str() {
                        if s.len() <= 80 {
                            return s.to_string();
                        }
                    }
                }
            }
            String::new()
        }
    }
}

/// Truncate a string to max visible characters, appending "…" if truncated.
fn truncate_line(line: &str, max_chars: usize) -> String {
    if line.chars().count() <= max_chars {
        return line.to_string();
    }
    let truncated: String = line.chars().take(max_chars).collect();
    format!("{}…", truncated)
}

/// Strip ANSI escape sequences for width calculation.
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_escape = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c == 'm' {
                in_escape = false;
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Check if a line is a code block line (4-space indented from markdown processing).
fn is_code_block_line(text: &str) -> bool {
    text.starts_with("    ") && !text.trim().is_empty()
}

/// Apply basic syntax highlighting to a code line using ANSI escape codes.
fn highlight_code_line(line: &str) -> String {
    // One Dark theme colors
    const RESET: &str = "\x1b[0m";
    const KW: &str = "\x1b[38;2;198;120;221m";      // purple - keywords
    const STR: &str = "\x1b[38;2;152;195;121m";      // green - strings
    const CMT: &str = "\x1b[38;2;92;99;112m";        // gray - comments
    const NUM: &str = "\x1b[38;2;209;154;102m";      // orange - numbers
    const BASE: &str = "\x1b[38;2;171;178;191m";     // light gray - base

    let trimmed = line.trim();

    // Full-line comment
    if trimmed.starts_with('#')
        || trimmed.starts_with("//")
        || trimmed.starts_with("--")
        || trimmed.starts_with("/*")
    {
        return format!("{}{}{}", CMT, line, RESET);
    }

    if trimmed.is_empty() {
        return line.to_string();
    }

    let mut result = String::with_capacity(line.len() * 2);
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;
    result.push_str(BASE);

    while i < len {
        // String literals
        if chars[i] == '"' || chars[i] == '\'' || chars[i] == '`' {
            let quote = chars[i];
            result.push_str(STR);
            result.push(quote);
            i += 1;
            while i < len && chars[i] != quote {
                if chars[i] == '\\' && i + 1 < len {
                    result.push(chars[i]);
                    result.push(chars[i + 1]);
                    i += 2;
                } else {
                    result.push(chars[i]);
                    i += 1;
                }
            }
            if i < len {
                result.push(quote);
                i += 1;
            }
            result.push_str(RESET);
            result.push_str(BASE);
            continue;
        }

        // Inline comment (# or //)
        if chars[i] == '#' || (chars[i] == '/' && i + 1 < len && chars[i + 1] == '/') {
            result.push_str(CMT);
            while i < len {
                result.push(chars[i]);
                i += 1;
            }
            break;
        }

        // Words (identifiers/keywords)
        if chars[i].is_alphabetic() || chars[i] == '_' || chars[i] == '@' {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if is_code_keyword(&word) {
                result.push_str(KW);
                result.push_str(&word);
                result.push_str(RESET);
                result.push_str(BASE);
            } else {
                result.push_str(&word);
            }
            continue;
        }

        // Numbers
        if chars[i].is_ascii_digit() {
            result.push_str(NUM);
            while i < len && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == 'x') {
                result.push(chars[i]);
                i += 1;
            }
            result.push_str(RESET);
            result.push_str(BASE);
            continue;
        }

        result.push(chars[i]);
        i += 1;
    }

    result.push_str(RESET);
    result
}

fn is_code_keyword(word: &str) -> bool {
    matches!(word,
        // Python
        "def" | "class" | "if" | "elif" | "else" | "for" | "while" | "return" |
        "import" | "from" | "with" | "try" | "except" | "finally" |
        "raise" | "pass" | "break" | "continue" | "and" | "or" | "not" |
        "None" | "True" | "False" | "self" | "async" | "await" |
        "yield" | "lambda" | "in" | "is" | "as" |
        // JavaScript/TypeScript
        "const" | "let" | "var" | "function" | "new" | "this" | "typeof" |
        "instanceof" | "export" | "default" | "switch" | "case" |
        "null" | "undefined" | "true" | "false" | "throw" | "catch" |
        "extends" | "implements" | "interface" | "readonly" | "abstract" |
        // Rust
        "fn" | "mut" | "pub" | "struct" | "enum" | "impl" | "trait" |
        "use" | "mod" | "match" | "crate" | "super" | "move" | "dyn" |
        "unsafe" | "extern" | "ref" | "where" | "type" |
        // Go
        "func" | "package" | "defer" | "chan" | "select" | "range" |
        // Common
        "void" | "static" | "final" | "private" | "protected" | "public" |
        "override" | "do" | "int" | "string" | "bool" | "float"
    )
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}

/// Convert markdown text to clean plain text with minimal ANSI formatting.
/// Handles: **bold**, `code`, [links](url), lists, headers, horizontal rules,
/// code blocks, tables, ordered lists.
fn markdown_to_plain(text: &str) -> String {
    let mut result = String::new();
    let mut in_code_block = false;
    let mut table_rows: Vec<Vec<String>> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();

        // Flush pending table if this line is not a table row
        let is_table_line = !in_code_block
            && trimmed.starts_with('|')
            && trimmed.ends_with('|')
            && trimmed.len() > 1;
        if !table_rows.is_empty() && !is_table_line {
            result.push_str(&render_table(&table_rows));
            table_rows.clear();
        }

        if line.starts_with("```") {
            in_code_block = !in_code_block;
            if in_code_block {
                // No opening border — just a blank separator line
                result.push('\n');
            } else {
                // No closing border — just a blank line
                result.push('\n');
            }
            continue;
        }
        if in_code_block {
            result.push_str(&format!("    {}\n", line));
            continue;
        }

        // Table lines
        if is_table_line {
            let inner = &trimmed[1..trimmed.len() - 1];
            let is_separator = inner.chars().all(|c| c == '-' || c == ':' || c == '|' || c == ' ');
            if !is_separator {
                let cells: Vec<String> = inner
                    .split('|')
                    .map(|c| strip_inline_md(c.trim()))
                    .collect();
                table_rows.push(cells);
            }
            continue;
        }

        // Horizontal rule
        if (trimmed.starts_with("---") || trimmed.starts_with("***") || trimmed.starts_with("___"))
            && trimmed.len() >= 3
            && trimmed.chars().all(|c| c == '-' || c == '*' || c == '_' || c == ' ')
        {
            result.push_str("────────────────────────────────\n\n");
            continue;
        }

        // Headers — add blank line before for visual separation
        if trimmed.starts_with("#### ") {
            if !result.is_empty() && !result.ends_with("\n\n") {
                result.push('\n');
            }
            result.push_str(&format!("  ▹ {}\n", strip_inline_md(&trimmed[5..])));
            continue;
        }
        if trimmed.starts_with("### ") {
            if !result.is_empty() && !result.ends_with("\n\n") {
                result.push('\n');
            }
            result.push_str(&format!("▸ {}\n", strip_inline_md(&trimmed[4..])));
            continue;
        }
        if trimmed.starts_with("## ") {
            if !result.is_empty() && !result.ends_with("\n\n") {
                result.push('\n');
            }
            result.push_str(&format!("── {}\n\n", strip_inline_md(&trimmed[3..])));
            continue;
        }
        if trimmed.starts_with("# ") {
            if !result.is_empty() && !result.ends_with("\n\n") {
                result.push('\n');
            }
            result.push_str(&format!("━━ {}\n\n", strip_inline_md(&trimmed[2..])));
            continue;
        }

        // Task lists
        if trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
            result.push_str(&format!("☑ {}\n", strip_inline_md(&trimmed[6..])));
            continue;
        }
        if trimmed.starts_with("- [ ] ") {
            result.push_str(&format!("☐ {}\n", strip_inline_md(&trimmed[6..])));
            continue;
        }

        // Unordered lists — preserve source indentation + base indent
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let src_indent = line.len() - line.trim_start().len();
            let pad = " ".repeat(2 + src_indent);
            result.push_str(&format!("{}• {}\n", pad, strip_inline_md(&trimmed[2..])));
            continue;
        }

        // Ordered lists (1. item, 2. item, etc.)
        if let Some(dot_pos) = trimmed.find(". ") {
            if dot_pos <= 3 && dot_pos > 0 && trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit()) {
                let num = &trimmed[..dot_pos];
                let content = &trimmed[dot_pos + 2..];
                let src_indent = line.len() - line.trim_start().len();
                let pad = " ".repeat(2 + src_indent);
                result.push_str(&format!("{}{}. {}\n", pad, num, strip_inline_md(content)));
                continue;
            }
        }

        // Blockquotes
        if trimmed.starts_with("> ") {
            result.push_str(&format!("▎ {}\n", strip_inline_md(&trimmed[2..])));
            continue;
        }

        // Regular line — strip inline markdown
        result.push_str(&strip_inline_md(line));
        result.push('\n');
    }

    // Flush remaining table
    if !table_rows.is_empty() {
        result.push_str(&render_table(&table_rows));
    }

    // Remove trailing newline
    if result.ends_with('\n') {
        result.pop();
    }
    result
}

/// Render markdown table rows as aligned text with box-drawing separators.
fn render_table(rows: &[Vec<String>]) -> String {
    use unicode_width::UnicodeWidthStr;

    if rows.is_empty() {
        return String::new();
    }

    let num_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut widths = vec![0usize; num_cols];
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < num_cols {
                widths[i] = widths[i].max(UnicodeWidthStr::width(cell.as_str()));
            }
        }
    }

    let mut result = String::new();

    for (row_idx, row) in rows.iter().enumerate() {
        result.push_str("  ");
        for (i, cell) in row.iter().enumerate() {
            if i >= num_cols {
                break;
            }
            let w = widths[i];
            let cell_w = UnicodeWidthStr::width(cell.as_str());
            let pad = w.saturating_sub(cell_w);
            if i > 0 {
                result.push_str(" │ ");
            }
            result.push_str(cell);
            result.push_str(&" ".repeat(pad));
        }
        result.push('\n');

        // Separator after header row
        if row_idx == 0 && rows.len() > 1 {
            result.push_str("  ");
            for (i, w) in widths.iter().enumerate() {
                if i > 0 {
                    result.push_str("─┼─");
                }
                result.push_str(&"─".repeat(*w));
            }
            result.push('\n');
        }
    }

    result
}

/// Process a single line of markdown for streaming output.
/// `in_code_block` tracks whether we're inside a ``` block.
fn process_md_line(line: &str, in_code_block: &mut bool) -> String {
    let trimmed = line.trim();

    // Code block fence
    if trimmed.starts_with("```") {
        *in_code_block = !*in_code_block;
        // No border — just return empty line as separator
        return String::new();
    }

    // Inside code block: indent with spaces (no border)
    if *in_code_block {
        return format!("    {}", line);
    }

    // Horizontal rule
    if (trimmed.starts_with("---") || trimmed.starts_with("***"))
        && trimmed.len() >= 3
        && trimmed.chars().all(|c| c == '-' || c == '*' || c == '_' || c == ' ')
    {
        return "────────────────────────────────".to_string();
    }
    // Table line (streaming — can't buffer, just clean up)
    if trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() > 1 {
        let inner = &trimmed[1..trimmed.len() - 1];
        let is_separator = inner.chars().all(|c| c == '-' || c == ':' || c == '|' || c == ' ');
        if is_separator {
            return "  ──────────────────────────".to_string();
        }
        let cells: Vec<String> = inner
            .split('|')
            .map(|c| strip_inline_md(c.trim()))
            .collect();
        return format!("  {}", cells.join("  │  "));
    }
    // Unordered list — preserve indentation
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        let src_indent = line.len() - line.trim_start().len();
        let pad = " ".repeat(2 + src_indent);
        return format!("{}• {}", pad, strip_inline_md(&trimmed[2..]));
    }
    // Ordered list
    if let Some(dot_pos) = trimmed.find(". ") {
        if dot_pos <= 3 && dot_pos > 0 && trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit()) {
            let num = &trimmed[..dot_pos];
            let content = &trimmed[dot_pos + 2..];
            let src_indent = line.len() - line.trim_start().len();
            let pad = " ".repeat(2 + src_indent);
            return format!("{}{}. {}", pad, num, strip_inline_md(content));
        }
    }
    // Headers (#### → H4 as sub-bullet, ### → H3, ## → H2, # → H1)
    if trimmed.starts_with("#### ") { return format!("  ▹ {}", strip_inline_md(&trimmed[5..])); }
    if trimmed.starts_with("### ") { return format!("▸ {}", strip_inline_md(&trimmed[4..])); }
    if trimmed.starts_with("## ") { return format!("── {}", strip_inline_md(&trimmed[3..])); }
    if trimmed.starts_with("# ") { return format!("━━ {}", strip_inline_md(&trimmed[2..])); }
    // Blockquote
    if trimmed.starts_with("> ") { return format!("▎ {}", strip_inline_md(&trimmed[2..])); }
    // Default: strip inline formatting
    strip_inline_md(line)
}

/// Strip inline markdown formatting: **bold** → bold, `code` → code, [text](url) → text
fn strip_inline_md(text: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if i + 1 < len && chars[i] == '*' && chars[i + 1] == '*' {
            // **bold** — find closing **
            i += 2;
            let start = i;
            while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '*') {
                i += 1;
            }
            let content: String = chars[start..i].iter().collect();
            result.push_str(&content);
            if i + 1 < len { i += 2; } // skip closing **
        } else if chars[i] == '`' {
            // `code` — find closing `
            i += 1;
            let start = i;
            while i < len && chars[i] != '`' {
                i += 1;
            }
            let content: String = chars[start..i].iter().collect();
            result.push_str(&content);
            if i < len { i += 1; } // skip closing `
        } else if chars[i] == '[' {
            // [text](url) → text
            let bracket_start = i + 1;
            let mut j = bracket_start;
            while j < len && chars[j] != ']' {
                j += 1;
            }
            if j + 1 < len && chars[j] == ']' && chars[j + 1] == '(' {
                let link_text: String = chars[bracket_start..j].iter().collect();
                // Skip past ](url)
                j += 2; // skip ](
                while j < len && chars[j] != ')' {
                    j += 1;
                }
                if j < len { j += 1; } // skip )
                result.push_str(&link_text);
                i = j;
            } else {
                result.push(chars[i]);
                i += 1;
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}
