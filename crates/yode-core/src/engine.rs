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
use yode_tools::tool::{SubAgentRunner, ToolContext, ToolResult, UserQuery};

use crate::context::AgentContext;
use crate::db::Database;
use crate::permission::{PermissionAction, PermissionManager};

/// Maximum size for tool results (50KB)
const MAX_TOOL_RESULT_SIZE: usize = 50 * 1024;

/// LLM call timeout in seconds
const LLM_TIMEOUT_SECS: u64 = 120;

/// Maximum retry count for network errors
const MAX_RETRIES: u32 = 3;

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
}

/// Response to a confirmation request.
#[derive(Debug, Clone)]
pub enum ConfirmResponse {
    Allow,
    Deny,
}

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
fn truncate_tool_result(result: ToolResult) -> ToolResult {
    if result.content.len() > MAX_TOOL_RESULT_SIZE {
        let truncated: String = result.content.chars().take(MAX_TOOL_RESULT_SIZE).collect();
        ToolResult {
            content: format!("{}...\n\n(结果已截断，原始大小: {} 字节)", truncated, result.content.len()),
            is_error: result.is_error,
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
        }
    }

    /// Set the database for session persistence.
    pub fn set_database(&mut self, db: Database) {
        self.db = Some(db);
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

            // If there are tool calls, execute them
            if !response.message.tool_calls.is_empty() {
                for tool_call in &response.message.tool_calls {
                    let result = self
                        .handle_tool_call(tool_call, &event_tx, &mut confirm_rx)
                        .await?;

                    // Truncate large results
                    let result = truncate_tool_result(result);

                    // Add tool result to messages
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
                                Some(ev) => Self::process_stream_event(ev, &mut full_text, &mut tool_calls, &mut final_response, &event_tx),
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
                        Some(ev) => Self::process_stream_event(ev, &mut full_text, &mut tool_calls, &mut final_response, &event_tx),
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
                    };
                    self.messages.push(assistant_msg);
                    self.persist_message("assistant", Some(&full_text), None, None);
                }
                let _ = event_tx.send(EngineEvent::Done);
                return Ok(());
            }

            // Wait for stream task
            if !cancelled {
                let _ = stream_handle.await;
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

            // Handle tool calls
            if !tool_calls.is_empty() {
                for tool_call in &tool_calls {
                    // Check cancellation before each tool
                    if let Some(ref token) = cancel_token {
                        if token.is_cancelled() {
                            let _ = event_tx.send(EngineEvent::Done);
                            return Ok(());
                        }
                    }

                    let result = self
                        .handle_tool_call(tool_call, &event_tx, &mut confirm_rx)
                        .await?;

                    // Truncate large results
                    let result = truncate_tool_result(result);

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

    /// Handle a single tool call, including permission checks.
    async fn handle_tool_call(
        &self,
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
        let params: serde_json::Value = serde_json::from_str(&tool_call.arguments)
            .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));

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

        // Execute the tool
        let ctx = self.build_tool_context();
        match tool.execute(params, &ctx).await {
            Ok(result) => Ok(result),
            Err(e) => {
                error!("Tool {} failed: {}", tool_call.name, e);
                Ok(ToolResult::error(format!("Tool execution failed: {}", e)))
            }
        }
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
