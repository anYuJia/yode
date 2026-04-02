use std::pin::Pin;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use yode_llm::provider::LlmProvider;
use yode_llm::types::{
    ChatRequest, ChatResponse, Message, Role, StreamEvent, ToolCall,
    ToolDefinition as LlmToolDefinition,
};
use yode_tools::registry::ToolRegistry;
use yode_tools::state::TaskStore;
use yode_tools::tool::{SubAgentRunner, ToolContext, ToolErrorType, ToolResult, UserQuery};
use yode_tools::validation;

use crate::context::AgentContext;
use crate::context_manager::ContextManager;
use crate::db::Database;
use crate::permission::{PermissionAction, PermissionManager};

/// Maximum size for tool results (50KB)
const MAX_TOOL_RESULT_SIZE: usize = 50 * 1024;

/// LLM call timeout in seconds
const LLM_TIMEOUT_SECS: u64 = 120;

/// Maximum retry count for network errors
const MAX_RETRIES: u32 = 3;

/// Per-tool timeout for parallel execution (30 seconds)
const PARALLEL_TOOL_TIMEOUT_SECS: u64 = 30;

/// Events emitted by the engine for the UI to consume.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// LLM is thinking (stream started)
    Thinking,
    /// Incremental text from LLM
    TextDelta(String),
    /// LLM produced a complete text response
    TextComplete(String),
    /// Tool call initiated
    ToolCallStart {
        id: String,
        name: String,
        arguments: String,
    },
    /// Tool requires user confirmation
    ToolConfirmRequired {
        id: String,
        name: String,
        arguments: String,
    },
    /// Tool execution result
    ToolResult {
        id: String,
        name: String,
        result: ToolResult,
    },
    /// Complete response from one LLM turn
    TurnComplete(ChatResponse),
    /// Error occurred
    Error(String),
    /// Tool is asking user a question
    AskUser {
        id: String,
        question: String,
    },
    /// Agent loop finished (no more tool calls)
    Done,
    /// Sub-agent started
    SubAgentStart { description: String },
    /// Sub-agent completed
    SubAgentComplete { result: String },
    /// Plan mode entered
    PlanModeEntered,
    /// Plan mode requires user approval
    PlanApprovalRequired { plan_content: String },
    /// Plan mode exited
    PlanModeExited,
    /// Context window was compressed to fit within limits
    ContextCompressed { removed: usize },
}

/// Response to a confirmation request.
#[derive(Debug, Clone)]
pub enum ConfirmResponse {
    Allow,
    Deny,
}

/// Tool call budget thresholds for analysis guidance.
const TOOL_BUDGET_NOTICE: u32 = 15;
const TOOL_BUDGET_WARNING: u32 = 25;
/// Self-reflection injection point.
const TOOL_REFLECT_INTERVAL: u32 = 10;
/// Goal reminder injection point.
const TOOL_GOAL_REMINDER: u32 = 5;

/// The core agent engine that drives the conversation loop.
pub struct AgentEngine {
    provider: Arc<dyn LlmProvider>,
    tools: Arc<ToolRegistry>,
    permissions: PermissionManager,
    context: AgentContext,
    messages: Vec<Message>,
    #[allow(dead_code)]
    system_prompt: String,
    db: Option<Database>,
    /// Shared task store for the todo tool.
    task_store: Arc<Mutex<TaskStore>>,
    /// Channel for ask_user questions (engine → TUI).
    ask_user_tx: Option<mpsc::UnboundedSender<UserQuery>>,
    /// Channel for ask_user answers (TUI → engine).
    ask_user_rx: Option<Arc<Mutex<mpsc::UnboundedReceiver<String>>>>,
    /// Tool call counter for the current turn (budget tracking).
    tool_call_count: u32,
    /// Recent tool call signatures for dedup detection (name+args hash).
    recent_tool_calls: Vec<(String, String)>,
    /// Consecutive tool call failure counter.
    consecutive_failures: u32,
    /// Context window manager for automatic compression.
    context_manager: ContextManager,
    /// Files the agent has already read in this turn (path → line count).
    files_read: std::collections::HashMap<String, usize>,
    /// Files the agent has modified in this turn.
    files_modified: Vec<String>,
}

/// Convert yode-tools ToolDefinition to yode-llm ToolDefinition.
fn convert_tool_definitions(registry: &ToolRegistry) -> Vec<LlmToolDefinition> {
    registry
        .definitions()
        .into_iter()
        .map(|td| LlmToolDefinition {
            name: td.name,
            description: td.description,
            parameters: td.parameters,
        })
        .collect()
}

/// Truncate tool result if it exceeds the size limit.
/// Preserves the beginning and end of the result for better context.
fn truncate_tool_result(result: ToolResult) -> ToolResult {
    if result.content.len() > MAX_TOOL_RESULT_SIZE {
        let head_size = MAX_TOOL_RESULT_SIZE * 3 / 4; // 75% from start
        let tail_size = MAX_TOOL_RESULT_SIZE / 4;       // 25% from end
        let head: String = result.content.chars().take(head_size).collect();
        let tail: String = result.content.chars().rev().take(tail_size).collect::<String>().chars().rev().collect();
        ToolResult {
            content: format!(
                "{}\n\n... [截断: 原始 {} 字节，使用 read_file 的 offset/limit 查看完整内容] ...\n\n{}",
                head,
                result.content.len(),
                tail
            ),
            is_error: result.is_error,
            error_type: result.error_type,
            recoverable: result.recoverable,
            suggestion: result.suggestion,
        }
    } else {
        result
    }
}

impl AgentEngine {
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        tools: Arc<ToolRegistry>,
        permissions: PermissionManager,
        context: AgentContext,
    ) -> Self {
        let mut system_prompt = include_str!("../../../prompts/system.md").to_string();

        // Inject runtime environment info
        system_prompt.push_str("\n\n# Environment\n\n");
        system_prompt.push_str(&format!(
            "- Working directory: {}\n- Platform: {} ({})\n- Date: {}\n- Model: {}\n- Provider: {}\n",
            context.working_dir.display(),
            std::env::consts::OS,
            std::env::consts::ARCH,
            chrono::Local::now().format("%Y-%m-%d"),
            context.model,
            context.provider,
        ));

        // Inject git status if in a git repo
        if context.working_dir.join(".git").exists() {
            system_prompt.push_str("- Git repo: yes\n");
            if let Ok(output) = std::process::Command::new("git")
                .args(["branch", "--show-current"])
                .current_dir(&context.working_dir)
                .output()
            {
                let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !branch.is_empty() {
                    system_prompt.push_str(&format!("- Branch: {}\n", branch));
                }
            }
        }

        // Try to load project-level YODE.md override
        let yode_md = context.working_dir.join("YODE.md");
        if yode_md.exists() {
            if let Ok(override_prompt) = std::fs::read_to_string(&yode_md) {
                system_prompt.push_str("\n\n# Project-specific instructions\n\n");
                system_prompt.push_str(&override_prompt);
                info!("Loaded project YODE.md from {}", yode_md.display());
            }
        }

        let mut messages = Vec::new();
        messages.push(Message::system(&system_prompt));

        let context_manager = ContextManager::new(&context.model);

        Self {
            provider,
            tools,
            permissions,
            context,
            messages,
            system_prompt,
            db: None,
            task_store: Arc::new(Mutex::new(TaskStore::new())),
            ask_user_tx: None,
            ask_user_rx: None,
            tool_call_count: 0,
            recent_tool_calls: Vec::new(),
            consecutive_failures: 0,
            context_manager,
            files_read: std::collections::HashMap::new(),
            files_modified: Vec::new(),
        }
    }

    /// Set the database for session persistence.
    pub fn set_database(&mut self, db: Database) {
        self.db = Some(db);
    }

    /// Switch the model at runtime.
    pub fn set_model(&mut self, model: String) {
        self.context.model = model;
    }

    /// Switch the provider at runtime.
    pub fn set_provider(&mut self, provider: Arc<dyn LlmProvider>, name: String) {
        self.provider = provider;
        self.context.provider = name;
    }

    /// Set channels for the ask_user tool.
    pub fn set_ask_user_channels(
        &mut self,
        tx: mpsc::UnboundedSender<UserQuery>,
        rx: mpsc::UnboundedReceiver<String>,
    ) {
        self.ask_user_tx = Some(tx);
        self.ask_user_rx = Some(Arc::new(Mutex::new(rx)));
    }

    /// Build a ToolContext with access to shared resources.
    fn build_tool_context(&self) -> ToolContext {
        ToolContext {
            registry: Some(Arc::clone(&self.tools)),
            tasks: Some(Arc::clone(&self.task_store)),
            user_input_tx: self.ask_user_tx.clone(),
            user_input_rx: self.ask_user_rx.clone(),
            working_dir: Some(self.context.working_dir.clone()),
            sub_agent_runner: None,
            mcp_resources: None,
            cron_manager: None,
            lsp_manager: None,
            worktree_state: None,
            plan_mode: None,
        }
    }

    /// Get the current message history.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Get the context.
    pub fn context(&self) -> &AgentContext {
        &self.context
    }

    /// Restore messages from database for a resumed session.
    pub fn restore_messages(&mut self, messages: Vec<Message>) {
        // Keep the system prompt as the first message, then append restored messages
        let system_msg = self.messages.first().cloned();
        self.messages.clear();
        if let Some(sys) = system_msg {
            self.messages.push(sys);
        }
        self.messages.extend(messages);
        info!("Restored {} messages from database", self.messages.len() - 1);
    }

    /// Save a message to the database if available.
    fn persist_message(&self, role: &str, content: Option<&str>, tool_calls_json: Option<&str>, tool_call_id: Option<&str>) {
        if let Some(ref db) = self.db {
            if let Err(e) = db.save_message(&self.context.session_id, role, content, tool_calls_json, tool_call_id) {
                warn!("Failed to persist message: {}", e);
            }
            if let Err(e) = db.touch_session(&self.context.session_id) {
                warn!("Failed to touch session: {}", e);
            }
        }
    }

    /// Run one user turn: send user message, loop through tool calls until final text response.
    pub async fn run_turn(
        &mut self,
        user_input: &str,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
        mut confirm_rx: mpsc::UnboundedReceiver<ConfirmResponse>,
    ) -> Result<()> {
        // Add user message
        self.messages.push(Message::user(user_input));
        self.persist_message("user", Some(user_input), None, None);

        // Reset tool call budget counter for this turn
        self.tool_call_count = 0;
        self.recent_tool_calls.clear();
        self.consecutive_failures = 0;
        self.files_read.clear();
        self.files_modified.clear();

        loop {
            let _ = event_tx.send(EngineEvent::Thinking);

            // Build chat request
            let request = ChatRequest {
                model: self.context.model.clone(),
                messages: self.messages.clone(),
                tools: convert_tool_definitions(&self.tools),
                temperature: Some(0.7),
                max_tokens: Some(4096),
            };

            // Call LLM with timeout and retry
            let response = self.call_llm_with_retry(request).await?;

            // Check if context window needs compression (should_compress caches token count)
            if self.context_manager.should_compress(response.usage.prompt_tokens, &self.messages) {
                let removed = self.context_manager.compress(&mut self.messages);
                if removed > 0 {
                    let _ = event_tx.send(EngineEvent::ContextCompressed { removed });
                }
            }

            debug!(
                "LLM response: text={:?}, tool_calls={}",
                response.message.content.as_deref().unwrap_or(""),
                response.message.tool_calls.len()
            );

            // Add assistant message to history
            self.messages.push(response.message.clone());

            // Persist assistant message
            let tc_json = if !response.message.tool_calls.is_empty() {
                serde_json::to_string(&response.message.tool_calls).ok()
            } else {
                None
            };
            self.persist_message(
                "assistant",
                response.message.content.as_deref(),
                tc_json.as_deref(),
                None,
            );

            // If there are tool calls, execute them (parallel where possible)
            if !response.message.tool_calls.is_empty() {
                let (parallel, sequential) = self.partition_tool_calls(&response.message.tool_calls);

                // Execute parallel tools concurrently
                let parallel_results = if !parallel.is_empty() {
                    info!("Executing {} tools in parallel", parallel.len());
                    self.execute_tools_parallel(&parallel, &event_tx).await
                } else {
                    vec![]
                };

                // Process parallel results
                for (tc, result) in &parallel_results {
                    let mut result = truncate_tool_result(result.clone());

                    self.inject_intelligence(&mut result, &tc.name, &tc.arguments);

                    self.messages.push(Message::tool_result(&tc.id, &result.content));
                    self.persist_message("tool", Some(&result.content), None, Some(&tc.id));

                    let _ = event_tx.send(EngineEvent::ToolResult {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        result,
                    });
                }

                // Execute sequential tools one by one
                for tool_call in &sequential {
                    let result = self
                        .handle_tool_call(tool_call, &event_tx, &mut confirm_rx)
                        .await?;

                    let mut result = truncate_tool_result(result);

                    self.inject_intelligence(&mut result, &tool_call.name, &tool_call.arguments);

                    self.messages.push(Message::tool_result(&tool_call.id, &result.content));
                    self.persist_message("tool", Some(&result.content), None, Some(&tool_call.id));

                    let _ = event_tx.send(EngineEvent::ToolResult {
                        id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        result,
                    });
                }

                continue;
            }

            // No tool calls — this is a text response, we're done
            if let Some(text) = &response.message.content {
                let _ = event_tx.send(EngineEvent::TextComplete(text.clone()));
            }

            let _ = event_tx.send(EngineEvent::TurnComplete(response));
            let _ = event_tx.send(EngineEvent::Done);
            break;
        }

        Ok(())
    }

    /// Run one user turn with streaming LLM output.
    /// Accepts an optional CancellationToken for cooperative cancellation.
    pub async fn run_turn_streaming(
        &mut self,
        user_input: &str,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
        mut confirm_rx: mpsc::UnboundedReceiver<ConfirmResponse>,
        cancel_token: Option<CancellationToken>,
    ) -> Result<()> {
        self.messages.push(Message::user(user_input));
        self.persist_message("user", Some(user_input), None, None);

        // Reset tool call budget counter for this turn
        self.tool_call_count = 0;
        self.recent_tool_calls.clear();
        self.consecutive_failures = 0;
        self.files_read.clear();
        self.files_modified.clear();

        loop {
            // Check cancellation before each LLM call
            if let Some(ref token) = cancel_token {
                if token.is_cancelled() {
                    let _ = event_tx.send(EngineEvent::Done);
                    return Ok(());
                }
            }

            let _ = event_tx.send(EngineEvent::Thinking);

            let request = ChatRequest {
                model: self.context.model.clone(),
                messages: self.messages.clone(),
                tools: convert_tool_definitions(&self.tools),
                temperature: Some(0.7),
                max_tokens: Some(4096),
            };

            // Stream LLM response with timeout
            let (stream_tx, mut stream_rx) = mpsc::channel::<StreamEvent>(256);

            let provider = self.provider.clone();
            let stream_handle = tokio::spawn(async move {
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(LLM_TIMEOUT_SECS),
                    provider.chat_stream(request, stream_tx),
                ).await;
                match result {
                    Ok(inner) => inner,
                    Err(_) => Err(anyhow::anyhow!("LLM 调用超时 ({}秒)", LLM_TIMEOUT_SECS)),
                }
            });

            let mut full_text = String::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();
            let mut final_response: Option<ChatResponse> = None;
            let mut cancelled = false;

            loop {
                if let Some(ref token) = cancel_token {
                    tokio::select! {
                        event = stream_rx.recv() => {
                            match event {
                                Some(ev) => {
                                    let is_done = matches!(ev, StreamEvent::Done(_));
                                    Self::process_stream_event(ev, &mut full_text, &mut tool_calls, &mut final_response, &event_tx);
                                    if is_done { break; }
                                }
                                None => break,
                            }
                        }
                        _ = token.cancelled() => {
                            cancelled = true;
                            stream_handle.abort();
                            break;
                        }
                    }
                } else {
                    match stream_rx.recv().await {
                        Some(ev) => {
                            let is_done = matches!(ev, StreamEvent::Done(_));
                            Self::process_stream_event(ev, &mut full_text, &mut tool_calls, &mut final_response, &event_tx);
                            if is_done { break; }
                        }
                        None => break,
                    }
                }
            }

            if cancelled {
                // Save partial text if any
                if !full_text.is_empty() {
                    let assistant_msg = Message {
                        role: Role::Assistant,
                        content: Some(full_text.clone()),
                        tool_calls: vec![],
                        tool_call_id: None,
                        images: Vec::new(),
                    };
                    self.messages.push(assistant_msg);
                    self.persist_message("assistant", Some(&full_text), None, None);
                }
                let _ = event_tx.send(EngineEvent::Done);
                return Ok(());
            }

            // Wait for stream task and check for errors
            if !cancelled {
                let stream_result = stream_handle.await;
                let stream_failed = match stream_result {
                    Ok(Ok(())) => false,
                    Ok(Err(e)) => {
                        warn!("Stream failed: {}", e);
                        let _ = event_tx.send(EngineEvent::Error(format!("Stream error: {}", e)));
                        true
                    }
                    Err(e) => {
                        warn!("Stream task panicked: {}", e);
                        let _ = event_tx.send(EngineEvent::Error(format!("Stream task error: {}", e)));
                        true
                    }
                };

                // Fallback: if stream failed with no content, retry non-streaming
                if stream_failed && full_text.is_empty() && tool_calls.is_empty() {
                    info!("Falling back to non-streaming LLM call");
                    let fallback_request = ChatRequest {
                        model: self.context.model.clone(),
                        messages: self.messages.clone(),
                        tools: convert_tool_definitions(&self.tools),
                        temperature: Some(0.7),
                        max_tokens: Some(4096),
                    };
                    match self.call_llm_with_retry(fallback_request).await {
                        Ok(response) => {
                            if let Some(ref text) = response.message.content {
                                full_text = text.clone();
                            }
                            tool_calls = response.message.tool_calls.clone();
                            final_response = Some(response);
                        }
                        Err(e) => {
                            error!("Non-streaming fallback also failed: {}", e);
                            let _ = event_tx.send(EngineEvent::Error(format!("LLM call failed: {}", e)));
                            let _ = event_tx.send(EngineEvent::Done);
                            return Err(e);
                        }
                    }
                }
                // If stream failed but we have partial content, keep it and continue
            }

            // Check if context window needs compression based on final response usage
            if let Some(ref resp) = final_response {
                if self.context_manager.should_compress(resp.usage.prompt_tokens, &self.messages) {
                    let removed = self.context_manager.compress(&mut self.messages);
                    if removed > 0 {
                        let _ = event_tx.send(EngineEvent::ContextCompressed { removed });
                    }
                }
            }

            // Build assistant message from stream
            let assistant_msg = Message {
                role: Role::Assistant,
                content: if full_text.is_empty() {
                    None
                } else {
                    Some(full_text.clone())
                },
                tool_calls: tool_calls.clone(),
                tool_call_id: None,
                images: Vec::new(),
            };
            self.messages.push(assistant_msg);

            // Persist assistant message
            let tc_json = if !tool_calls.is_empty() {
                serde_json::to_string(&tool_calls).ok()
            } else {
                None
            };
            self.persist_message(
                "assistant",
                if full_text.is_empty() { None } else { Some(&full_text) },
                tc_json.as_deref(),
                None,
            );

            // Handle tool calls (parallel where possible)
            if !tool_calls.is_empty() {
                let (parallel, sequential) = self.partition_tool_calls(&tool_calls);

                // Execute parallel tools concurrently
                if !parallel.is_empty() {
                    // Check cancellation before parallel batch
                    if let Some(ref token) = cancel_token {
                        if token.is_cancelled() {
                            let _ = event_tx.send(EngineEvent::Done);
                            return Ok(());
                        }
                    }

                    info!("Executing {} tools in parallel (streaming)", parallel.len());
                    let parallel_results = self.execute_tools_parallel(&parallel, &event_tx).await;

                    for (tc, result) in parallel_results {
                        let mut result = truncate_tool_result(result);

                        self.tool_call_count += 1;
                        if self.tool_call_count == TOOL_BUDGET_WARNING {
                            result.content.push_str("\n\n[Budget warning: 25 tool calls used. Stop exploring and produce your report.]");
                        } else if self.tool_call_count == TOOL_BUDGET_NOTICE {
                            result.content.push_str("\n\n[Budget notice: 15 tool calls used. Consider summarizing current findings before continuing.]");
                        }

                        self.messages.push(Message::tool_result(&tc.id, &result.content));
                        self.persist_message("tool", Some(&result.content), None, Some(&tc.id));

                        let _ = event_tx.send(EngineEvent::ToolResult {
                            id: tc.id.clone(),
                            name: tc.name.clone(),
                            result,
                        });
                    }
                }

                // Execute sequential tools one by one
                for tool_call in &sequential {
                    if let Some(ref token) = cancel_token {
                        if token.is_cancelled() {
                            let _ = event_tx.send(EngineEvent::Done);
                            return Ok(());
                        }
                    }

                    let result = self
                        .handle_tool_call(tool_call, &event_tx, &mut confirm_rx)
                        .await?;

                    let mut result = truncate_tool_result(result);

                    self.inject_intelligence(&mut result, &tool_call.name, &tool_call.arguments);

                    self.messages
                        .push(Message::tool_result(&tool_call.id, &result.content));
                    self.persist_message("tool", Some(&result.content), None, Some(&tool_call.id));

                    let _ = event_tx.send(EngineEvent::ToolResult {
                        id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        result,
                    });
                }
                continue;
            }

            // Done
            if !full_text.is_empty() {
                let _ = event_tx.send(EngineEvent::TextComplete(full_text));
            }
            if let Some(resp) = final_response {
                let _ = event_tx.send(EngineEvent::TurnComplete(resp));
            }
            let _ = event_tx.send(EngineEvent::Done);
            break;
        }

        Ok(())
    }

    /// Process a single stream event.
    fn process_stream_event(
        event: StreamEvent,
        full_text: &mut String,
        tool_calls: &mut Vec<ToolCall>,
        final_response: &mut Option<ChatResponse>,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) {
        match event {
            StreamEvent::TextDelta(delta) => {
                full_text.push_str(&delta);
                let _ = event_tx.send(EngineEvent::TextDelta(delta));
            }
            StreamEvent::ToolCallStart { id, name } => {
                tool_calls.push(ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: String::new(),
                });
                let _ = event_tx.send(EngineEvent::ToolCallStart {
                    id,
                    name,
                    arguments: String::new(),
                });
            }
            StreamEvent::ToolCallDelta { id, arguments } => {
                if let Some(tc) = tool_calls.iter_mut().find(|t| t.id == id) {
                    tc.arguments.push_str(&arguments);
                }
            }
            StreamEvent::ToolCallEnd { id: _ } => {}
            StreamEvent::Done(resp) => {
                *final_response = Some(resp);
            }
            StreamEvent::Error(e) => {
                let _ = event_tx.send(EngineEvent::Error(e));
            }
        }
    }

    /// Call LLM with retry logic for network errors (non-streaming).
    async fn call_llm_with_retry(&self, request: ChatRequest) -> Result<ChatResponse> {
        let mut last_err = None;
        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                let delay = std::time::Duration::from_millis(1000 * 2u64.pow(attempt - 1));
                info!("Retrying LLM call (attempt {}/{}), waiting {:?}", attempt + 1, MAX_RETRIES, delay);
                tokio::time::sleep(delay).await;
            }

            let result = tokio::time::timeout(
                std::time::Duration::from_secs(LLM_TIMEOUT_SECS),
                self.provider.chat(request.clone()),
            ).await;

            match result {
                Ok(Ok(response)) => return Ok(response),
                Ok(Err(e)) => {
                    warn!("LLM call failed (attempt {}): {}", attempt + 1, e);
                    last_err = Some(e);
                }
                Err(_) => {
                    let err = anyhow::anyhow!("LLM 调用超时 ({}秒)", LLM_TIMEOUT_SECS);
                    warn!("LLM call timed out (attempt {})", attempt + 1);
                    last_err = Some(err);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("LLM call failed after {} retries", MAX_RETRIES)))
            .context("LLM chat request failed")
    }

    /// Inject intelligence into tool results based on accumulated state.
    /// This is the core "thinking aid" — it gives the LLM the right context at the right time.
    fn inject_intelligence(&mut self, result: &mut ToolResult, tool_name: &str, tool_args: &str) {
        self.tool_call_count += 1;

        // Track consecutive failures
        if result.is_error {
            self.consecutive_failures += 1;
        } else {
            self.consecutive_failures = 0;
        }

        // === State tracking: remember what files we've seen/changed ===

        // Extract file_path from tool arguments if present
        let file_path = serde_json::from_str::<serde_json::Value>(tool_args)
            .ok()
            .and_then(|v| v.get("file_path").and_then(|p| p.as_str()).map(String::from));

        if let Some(ref path) = file_path {
            match tool_name {
                "read_file" if !result.is_error => {
                    let line_count = result.content.lines().count();
                    // Duplicate read detection
                    if let Some(&prev_lines) = self.files_read.get(path.as_str()) {
                        result.content.push_str(&format!(
                            "\n\n[Note: You already read this file earlier ({} lines). \
                             If you need specific lines, use offset/limit instead of re-reading.]",
                            prev_lines
                        ));
                    }
                    self.files_read.insert(path.clone(), line_count);
                }
                "edit_file" | "write_file" | "multi_edit" if !result.is_error => {
                    self.files_modified.push(path.clone());
                }
                _ => {}
            }
        }

        // === Contextual intelligence based on what just happened ===

        // After editing: suggest cross-reference check
        if !result.is_error && (tool_name == "edit_file" || tool_name == "write_file") {
            if self.files_modified.len() == 1 {
                // First edit — remind about verification
                result.content.push_str(
                    "\n\n[Next: Run `bash` with build command to verify. \
                     If you changed a function signature, grep for callers to update them too.]"
                );
            } else if self.files_modified.len() > 3 {
                // Many edits — remind to build
                result.content.push_str(&format!(
                    "\n\n[You've modified {} files so far. Run a build to catch any issues before continuing.]",
                    self.files_modified.len()
                ));
            }
        }

        // After bash: analyze build errors
        if tool_name == "bash" && result.is_error {
            // Look for common Rust compile error patterns
            if result.content.contains("error[E") {
                // Extract first error location
                if let Some(line) = result.content.lines().find(|l| l.contains("error[E")) {
                    result.content.push_str(&format!(
                        "\n\n[Build error detected. Focus on the first error: `{}`\n\
                         Read the file at the indicated line to understand the issue before attempting a fix.]",
                        line.trim().chars().take(200).collect::<String>()
                    ));
                }
            }
        }

        // Consecutive failures — escalating strategy change
        if self.consecutive_failures == 2 {
            result.content.push_str(
                "\n\n[2 failures in a row. Your current approach isn't working. \
                 Step back: What assumption might be wrong? Try a different tool or strategy.]"
            );
        } else if self.consecutive_failures >= 3 {
            result.content.push_str(
                "\n\n[3+ consecutive failures. STOP searching and summarize what you know. \
                 Present your findings to the user and ask for guidance.]"
            );
        }

        // === Periodic intelligence ===

        // Goal reminder at 5 calls
        if self.tool_call_count == TOOL_GOAL_REMINDER {
            result.content.push_str(
                "\n\n[5 tool calls done. Quick check: Do you have enough information to act? \
                 If yes, stop gathering and start implementing.]"
            );
        }

        // Self-reflection every 10 calls
        if self.tool_call_count > 0 && self.tool_call_count % TOOL_REFLECT_INTERVAL == 0 {
            let state_summary = format!(
                "\n\n[Checkpoint: {} tool calls | {} files read | {} files modified. \
                 Summarize your understanding. What's your hypothesis? What's the most efficient next step?]",
                self.tool_call_count,
                self.files_read.len(),
                self.files_modified.len()
            );
            result.content.push_str(&state_summary);
        }

        // Budget warnings
        if self.tool_call_count == TOOL_BUDGET_WARNING {
            result.content.push_str(
                "\n\n[Budget: 25 calls used. Produce your answer/fix NOW.]"
            );
        } else if self.tool_call_count == TOOL_BUDGET_NOTICE {
            result.content.push_str(
                "\n\n[Budget: 15 calls. Start converging toward a solution.]"
            );
        }
    }

    /// Partition tool calls into (parallel, sequential) based on permission and read_only.
    fn partition_tool_calls(&self, tool_calls: &[ToolCall]) -> (Vec<ToolCall>, Vec<ToolCall>) {
        let mut parallel = Vec::new();
        let mut sequential = Vec::new();

        for tc in tool_calls {
            let can_parallel = if let Some(tool) = self.tools.get(&tc.name) {
                let caps = tool.capabilities();
                caps.read_only && matches!(self.permissions.check(&tc.name), PermissionAction::Allow)
            } else {
                false
            };

            if can_parallel {
                parallel.push(tc.clone());
            } else {
                sequential.push(tc.clone());
            }
        }

        (parallel, sequential)
    }

    /// Execute a batch of read-only, auto-allowed tool calls in parallel.
    async fn execute_tools_parallel(
        &self,
        tool_calls: &[ToolCall],
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) -> Vec<(ToolCall, ToolResult)> {
        use futures::future::join_all;

        let mut futures = Vec::new();

        for tc in tool_calls {
            let tool = match self.tools.get(&tc.name) {
                Some(t) => t,
                None => continue,
            };

            let mut params: serde_json::Value = serde_json::from_str(&tc.arguments)
                .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));

            let schema = tool.parameters_schema();
            if let Err(msg) = validation::validate_and_coerce(&schema, &mut params) {
                let tc_clone = tc.clone();
                let result = ToolResult::error_typed(
                    format!("Parameter validation failed: {}", msg),
                    ToolErrorType::Validation,
                    true,
                    Some(format!("Fix the parameters and retry. Schema: {}", schema)),
                );
                futures.push(Box::pin(async move { (tc_clone, result) })
                    as Pin<Box<dyn std::future::Future<Output = (ToolCall, ToolResult)> + Send>>);
                continue;
            }

            let _ = event_tx.send(EngineEvent::ToolCallStart {
                id: tc.id.clone(),
                name: tc.name.clone(),
                arguments: tc.arguments.clone(),
            });

            info!("Executing tool in parallel: {} (auto-allowed, read-only)", tc.name);

            let ctx = self.build_tool_context();
            let tool_name = tc.name.clone();
            let tc_clone = tc.clone();

            futures.push(Box::pin(async move {
                let start = std::time::Instant::now();
                let timeout = std::time::Duration::from_secs(PARALLEL_TOOL_TIMEOUT_SECS);
                let result = match tokio::time::timeout(timeout, tool.execute(params, &ctx)).await {
                    Ok(Ok(r)) => r,
                    Ok(Err(e)) => {
                        error!("Tool {} failed: {}", tool_name, e);
                        ToolResult::error(format!("Tool execution failed: {}", e))
                    }
                    Err(_) => {
                        warn!("Tool {} timed out after {}s", tool_name, PARALLEL_TOOL_TIMEOUT_SECS);
                        ToolResult::error_typed(
                            format!("Tool {} timed out after {} seconds", tool_name, PARALLEL_TOOL_TIMEOUT_SECS),
                            ToolErrorType::Timeout,
                            true,
                            Some("Try a smaller scope or more specific parameters.".to_string()),
                        )
                    }
                };
                debug!(tool = %tool_name, elapsed_ms = start.elapsed().as_millis() as u64, "Parallel tool completed");
                (tc_clone, result)
            }));
        }

        join_all(futures).await
    }

    /// Handle a single tool call, including permission checks, dedup detection, and timing.
    async fn handle_tool_call(
        &mut self,
        tool_call: &ToolCall,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
        confirm_rx: &mut mpsc::UnboundedReceiver<ConfirmResponse>,
    ) -> Result<ToolResult> {
        let tool = match self.tools.get(&tool_call.name) {
            Some(t) => t,
            None => {
                return Ok(ToolResult::error(format!(
                    "Unknown tool: {}",
                    tool_call.name
                )));
            }
        };

        // Parse arguments
        let mut params: serde_json::Value = serde_json::from_str(&tool_call.arguments)
            .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));

        // Validate and coerce parameters against the tool's schema
        let schema = tool.parameters_schema();
        if let Err(msg) = validation::validate_and_coerce(&schema, &mut params) {
            return Ok(ToolResult::error_typed(
                format!("Parameter validation failed: {}", msg),
                ToolErrorType::Validation,
                true,
                Some(format!("Fix the parameters and retry. Schema: {}", schema)),
            ));
        }

        // Check permissions
        let action = self.permissions.check(&tool_call.name);

        match action {
            PermissionAction::Allow => {
                info!("Executing tool: {} (auto-allowed)", tool_call.name);
                // Re-send ToolCallStart with full arguments so TUI can update display.
                // (The initial ToolCallStart from streaming had empty arguments.)
                let _ = event_tx.send(EngineEvent::ToolCallStart {
                    id: tool_call.id.clone(),
                    name: tool_call.name.clone(),
                    arguments: tool_call.arguments.clone(),
                });
            }
            PermissionAction::Confirm => {
                let _ = event_tx.send(EngineEvent::ToolConfirmRequired {
                    id: tool_call.id.clone(),
                    name: tool_call.name.clone(),
                    arguments: tool_call.arguments.clone(),
                });

                match confirm_rx.recv().await {
                    Some(ConfirmResponse::Allow) => {
                        info!("Tool {} confirmed by user", tool_call.name);
                    }
                    Some(ConfirmResponse::Deny) => {
                        info!("Tool {} denied by user", tool_call.name);
                        return Ok(ToolResult::error(
                            "Tool execution denied by user.".to_string(),
                        ));
                    }
                    None => {
                        return Ok(ToolResult::error(
                            "Confirmation channel closed.".to_string(),
                        ));
                    }
                }
            }
            PermissionAction::Deny => {
                return Ok(ToolResult::error(format!(
                    "Tool {} is not permitted.",
                    tool_call.name
                )));
            }
        }

        // Dedup detection: warn if same tool+args was called recently
        let call_sig = (tool_call.name.clone(), tool_call.arguments.clone());
        if self.recent_tool_calls.contains(&call_sig) {
            return Ok(ToolResult::error_typed(
                format!(
                    "Duplicate tool call detected: {} was called with identical arguments. Change your approach instead of repeating the same call.",
                    tool_call.name
                ),
                ToolErrorType::Validation,
                true,
                Some("Try different parameters, a different tool, or summarize what you know so far.".to_string()),
            ));
        }
        self.recent_tool_calls.push(call_sig);
        // Keep only last 10 calls to avoid unbounded growth
        if self.recent_tool_calls.len() > 10 {
            self.recent_tool_calls.remove(0);
        }

        // Execute the tool with timing
        let start_time = std::time::Instant::now();
        let ctx = self.build_tool_context();
        let mut result = match tool.execute(params, &ctx).await {
            Ok(result) => result,
            Err(e) => {
                error!("Tool {} failed: {}", tool_call.name, e);
                ToolResult::error(format!("Tool execution failed: {}", e))
            }
        };
        let elapsed = start_time.elapsed();
        debug!(tool = %tool_call.name, elapsed_ms = elapsed.as_millis() as u64, "Tool execution completed");

        // Append recovery suggestion to content so LLM can see it
        if result.is_error {
            // Add contextual recovery hints based on error type
            let auto_hint = match result.error_type {
                Some(ToolErrorType::NotFound) => {
                    Some(format!(
                        "Try using `glob` to find the correct path, or `grep` to search for the symbol by name."
                    ))
                }
                Some(ToolErrorType::Validation) => {
                    Some(format!(
                        "Re-check parameter types and required fields. Schema: {}",
                        tool.parameters_schema()
                    ))
                }
                Some(ToolErrorType::Timeout) => {
                    Some("Reduce the scope of the operation (smaller file range, fewer results) and retry.".to_string())
                }
                Some(ToolErrorType::Permission) => {
                    Some("This operation requires user confirmation. The user denied it — try an alternative approach.".to_string())
                }
                _ => None,
            };

            // Prefer tool-provided suggestion, fall back to auto-generated hint
            if let Some(ref suggestion) = result.suggestion {
                result.content.push_str(&format!("\n\nSuggestion: {}", suggestion));
            } else if let Some(hint) = auto_hint {
                result.content.push_str(&format!("\n\nSuggestion: {}", hint));
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use yode_llm::types::ToolCall;
    use yode_tools::registry::ToolRegistry;
    use yode_tools::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

    /// Minimal mock LLM provider (never actually called in these tests).
    struct MockProvider;

    #[async_trait::async_trait]
    impl yode_llm::provider::LlmProvider for MockProvider {
        fn name(&self) -> &str { "mock" }
        async fn chat(&self, _req: yode_llm::types::ChatRequest) -> anyhow::Result<yode_llm::types::ChatResponse> {
            unimplemented!("Mock provider should not be called in unit tests")
        }
        async fn chat_stream(&self, _req: yode_llm::types::ChatRequest, _tx: tokio::sync::mpsc::Sender<yode_llm::types::StreamEvent>) -> anyhow::Result<()> {
            unimplemented!()
        }
        async fn list_models(&self) -> anyhow::Result<Vec<yode_llm::ModelInfo>> {
            Ok(vec![])
        }
    }

    /// A mock read-only tool for testing parallel execution.
    struct MockReadTool { name: String }

    #[async_trait::async_trait]
    impl Tool for MockReadTool {
        fn name(&self) -> &str { &self.name }
        fn description(&self) -> &str { "mock read tool" }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {}})
        }
        fn capabilities(&self) -> ToolCapabilities {
            ToolCapabilities {
                requires_confirmation: false,
                supports_auto_execution: true,
                read_only: true,
            }
        }
        async fn execute(&self, _params: serde_json::Value, _ctx: &ToolContext) -> anyhow::Result<ToolResult> {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            Ok(ToolResult::success(format!("result from {}", self.name)))
        }
    }

    /// A mock write tool that requires confirmation.
    struct MockWriteTool;

    #[async_trait::async_trait]
    impl Tool for MockWriteTool {
        fn name(&self) -> &str { "mock_write" }
        fn description(&self) -> &str { "mock write tool" }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {}})
        }
        fn capabilities(&self) -> ToolCapabilities {
            ToolCapabilities {
                requires_confirmation: true,
                supports_auto_execution: false,
                read_only: false,
            }
        }
        async fn execute(&self, _params: serde_json::Value, _ctx: &ToolContext) -> anyhow::Result<ToolResult> {
            Ok(ToolResult::success("write done".to_string()))
        }
    }

    fn make_engine(tools: Vec<Arc<dyn Tool>>, confirm_tools: Vec<String>) -> AgentEngine {
        let mut registry = ToolRegistry::new();
        for t in tools {
            registry.register(t);
        }
        let provider: Arc<dyn yode_llm::provider::LlmProvider> = Arc::new(MockProvider);
        let permissions = PermissionManager::new(confirm_tools);
        let context = AgentContext::new(
            std::env::current_dir().unwrap(),
            "mock".to_string(),
            "claude-sonnet-4".to_string(),
        );
        AgentEngine::new(provider, Arc::new(registry), permissions, context)
    }

    // --- partition_tool_calls tests ---

    #[test]
    fn test_partition_all_read_only() {
        let engine = make_engine(
            vec![
                Arc::new(MockReadTool { name: "r1".into() }),
                Arc::new(MockReadTool { name: "r2".into() }),
                Arc::new(MockReadTool { name: "r3".into() }),
            ],
            vec![],
        );
        let tcs = vec![
            ToolCall { id: "1".into(), name: "r1".into(), arguments: "{}".into() },
            ToolCall { id: "2".into(), name: "r2".into(), arguments: "{}".into() },
            ToolCall { id: "3".into(), name: "r3".into(), arguments: "{}".into() },
        ];
        let (par, seq) = engine.partition_tool_calls(&tcs);
        assert_eq!(par.len(), 3);
        assert_eq!(seq.len(), 0);
    }

    #[test]
    fn test_partition_mixed() {
        let engine = make_engine(
            vec![
                Arc::new(MockReadTool { name: "reader".into() }),
                Arc::new(MockWriteTool),
            ],
            vec!["mock_write".into()],
        );
        let tcs = vec![
            ToolCall { id: "1".into(), name: "reader".into(), arguments: "{}".into() },
            ToolCall { id: "2".into(), name: "mock_write".into(), arguments: "{}".into() },
            ToolCall { id: "3".into(), name: "reader".into(), arguments: "{}".into() },
        ];
        let (par, seq) = engine.partition_tool_calls(&tcs);
        assert_eq!(par.len(), 2);
        assert_eq!(seq.len(), 1);
        assert_eq!(seq[0].name, "mock_write");
    }

    #[test]
    fn test_partition_unknown_tool() {
        let engine = make_engine(vec![], vec![]);
        let tcs = vec![
            ToolCall { id: "1".into(), name: "nonexistent".into(), arguments: "{}".into() },
        ];
        let (par, seq) = engine.partition_tool_calls(&tcs);
        assert_eq!(par.len(), 0);
        assert_eq!(seq.len(), 1);
    }

    #[test]
    fn test_partition_read_only_needing_confirm() {
        let engine = make_engine(
            vec![Arc::new(MockReadTool { name: "sensitive".into() })],
            vec!["sensitive".into()],
        );
        let tcs = vec![
            ToolCall { id: "1".into(), name: "sensitive".into(), arguments: "{}".into() },
        ];
        let (par, seq) = engine.partition_tool_calls(&tcs);
        assert_eq!(par.len(), 0, "Confirm-required tools must not be parallelized");
        assert_eq!(seq.len(), 1);
    }

    // --- execute_tools_parallel tests ---

    #[tokio::test]
    async fn test_parallel_returns_all_results_in_order() {
        let engine = make_engine(
            vec![
                Arc::new(MockReadTool { name: "a".into() }),
                Arc::new(MockReadTool { name: "b".into() }),
                Arc::new(MockReadTool { name: "c".into() }),
            ],
            vec![],
        );
        let tcs = vec![
            ToolCall { id: "x1".into(), name: "a".into(), arguments: "{}".into() },
            ToolCall { id: "x2".into(), name: "b".into(), arguments: "{}".into() },
            ToolCall { id: "x3".into(), name: "c".into(), arguments: "{}".into() },
        ];
        let (tx, mut rx) = mpsc::unbounded_channel();
        let results = engine.execute_tools_parallel(&tcs, &tx).await;

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0.id, "x1");
        assert_eq!(results[1].0.id, "x2");
        assert_eq!(results[2].0.id, "x3");
        for (_tc, r) in &results {
            assert!(!r.is_error);
        }

        // Check events
        let mut starts = 0;
        while let Ok(ev) = rx.try_recv() {
            if matches!(ev, EngineEvent::ToolCallStart { .. }) { starts += 1; }
        }
        assert_eq!(starts, 3);
    }

    #[tokio::test]
    async fn test_parallel_empty() {
        let engine = make_engine(vec![], vec![]);
        let (tx, _rx) = mpsc::unbounded_channel();
        let results = engine.execute_tools_parallel(&[], &tx).await;
        assert!(results.is_empty());
    }
}

/// Implementation of SubAgentRunner that creates a fresh AgentEngine for each sub-agent.
pub struct SubAgentRunnerImpl {
    pub provider: Arc<dyn LlmProvider>,
    pub tools: Arc<ToolRegistry>,
    pub context: AgentContext,
}

impl SubAgentRunner for SubAgentRunnerImpl {
    fn run_sub_agent(
        &self,
        prompt: String,
        allowed_tools: Vec<String>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + '_>> {
        Box::pin(async move {
            // Create a filtered tool registry for the sub-agent
            let mut sub_registry = ToolRegistry::new();
            if allowed_tools.is_empty() {
                // Give all tools
                for tool in self.tools.list() {
                    sub_registry.register(tool);
                }
            } else {
                for name in &allowed_tools {
                    if let Some(tool) = self.tools.get(name) {
                        sub_registry.register(tool);
                    }
                }
            }

            let sub_registry = Arc::new(sub_registry);

            // Create a permissive permission manager for sub-agents
            let permissions = PermissionManager::new(vec![]); // auto-allow all

            // Create sub-agent engine
            let mut engine = AgentEngine::new(
                Arc::clone(&self.provider),
                sub_registry,
                permissions,
                self.context.clone(),
            );

            // Run non-streaming turn
            let (event_tx, mut event_rx) = mpsc::unbounded_channel();
            let (_confirm_tx, confirm_rx) = mpsc::unbounded_channel();

            engine.run_turn(&prompt, event_tx, confirm_rx).await?;

            // Collect final text from events
            let mut result_text = String::new();
            while let Ok(event) = event_rx.try_recv() {
                if let EngineEvent::TextComplete(text) = event {
                    result_text = text;
                }
            }

            if result_text.is_empty() {
                result_text = "Sub-agent completed without text output.".to_string();
            }

            Ok(result_text)
        })
    }
}
