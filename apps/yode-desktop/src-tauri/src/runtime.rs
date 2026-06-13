use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::json;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use uuid::Uuid;

use yode_core::config::Config;
use yode_core::context::AgentContext;
use yode_core::db::{Database, StoredMessage};
use yode_core::engine::{AgentEngine, ConfirmResponse, EngineEvent};
use yode_core::permission::{PermissionManager, PermissionRule, RuleBehavior, RuleSource};
use yode_core::session::Session;
use yode_llm::registry::ProviderRegistry;
use yode_tools::registry::ToolRegistry;
use yode_tools::tool::McpResourceProvider;

use crate::protocol::{
    Bootstrap, CreateSessionRequest, DefaultLlm, DesktopEvent, DesktopImageOutput, DesktopMessage,
    DesktopProvider, DesktopSession, RuntimeState, SendMessageRequest, TerminalRunRequest,
    TerminalRunResponse, TurnAccepted,
};

pub struct DesktopRuntime {
    config: Mutex<Config>,
    db: Database,
    db_path: PathBuf,
    workspace_path: PathBuf,
    provider_registry: Mutex<Arc<ProviderRegistry>>,
    tool_registry: Arc<ToolRegistry>,
    mcp_resource_provider: Option<Arc<dyn McpResourceProvider>>,
    active_session_id: Mutex<Option<String>>,
    permission_mode: Mutex<String>,
    seq: AtomicU64,
    confirm_txs: Arc<Mutex<HashMap<(String, String), UnboundedSender<ConfirmResponse>>>>,
    ask_user_txs: Arc<Mutex<HashMap<(String, String), UnboundedSender<String>>>>,
    cancel_tokens: Arc<Mutex<HashMap<(String, String), tokio_util::sync::CancellationToken>>>,
    pending_confirmations: Arc<Mutex<HashMap<(String, String), PendingConfirmation>>>,
    session_permission_rules: Arc<Mutex<HashMap<String, Vec<PermissionRule>>>>,
    terminal_sessions: Mutex<HashMap<String, TerminalSessionState>>,
}

#[derive(Debug, Clone)]
struct PendingConfirmation {
    tool_name: String,
    command: Option<String>,
}

#[derive(Debug, Clone)]
struct TerminalSessionState {
    cwd: PathBuf,
    env: HashMap<String, String>,
}

impl DesktopRuntime {
    pub fn new() -> Result<Self> {
        let workspace_path = resolve_desktop_workspace_path();
        let db_path = dirs::home_dir()
            .unwrap_or_else(|| workspace_path.clone())
            .join(".yode")
            .join("sessions.db");

        let config = Config::load()
            .unwrap_or_else(|_| Config::load_from(None).expect("failed to load default config"));

        let provider_registry = Mutex::new(bootstrap_providers(&config));
        let (tool_registry, mcp_resource_provider) =
            setup_desktop_tooling(&config, &workspace_path);

        let default_mode = config
            .permissions
            .default_mode
            .clone()
            .unwrap_or_else(|| "Default".to_string());

        Ok(Self {
            config: Mutex::new(config),
            db: Database::open(&db_path)?,
            db_path,
            workspace_path,
            provider_registry,
            tool_registry,
            mcp_resource_provider,
            active_session_id: Mutex::new(None),
            permission_mode: Mutex::new(default_mode),
            seq: AtomicU64::new(1),
            confirm_txs: Arc::new(Mutex::new(HashMap::new())),
            ask_user_txs: Arc::new(Mutex::new(HashMap::new())),
            cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
            pending_confirmations: Arc::new(Mutex::new(HashMap::new())),
            session_permission_rules: Arc::new(Mutex::new(HashMap::new())),
            terminal_sessions: Mutex::new(HashMap::new()),
        })
    }

    pub fn bootstrap(&self) -> Result<Bootstrap> {
        let sessions = self.sessions_list()?;
        let permission_mode = self
            .permission_mode
            .lock()
            .map_err(|_| anyhow::anyhow!("permission mode lock poisoned"))?
            .clone();
        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        Ok(Bootstrap {
            app_version: env!("CARGO_PKG_VERSION"),
            workspace_path: self.workspace_path.display().to_string(),
            provider: config.llm.default_provider.clone(),
            model: config.llm.default_model.clone(),
            permission_mode,
            sessions,
        })
    }

    pub fn sessions_list(&self) -> Result<Vec<DesktopSession>> {
        let active_session_id = self
            .active_session_id
            .lock()
            .map_err(|_| anyhow::anyhow!("active session lock poisoned"))?
            .clone();

        Ok(self
            .db
            .list_sessions(50)?
            .into_iter()
            .map(|session| self.map_session(session, active_session_id.as_deref()))
            .collect())
    }

    pub fn sessions_create(&self, request: CreateSessionRequest) -> Result<DesktopSession> {
        let now = Utc::now();
        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        let (default_provider, default_model) = self.default_llm_for_new_session(&config)?;
        let session = Session {
            id: Uuid::new_v4().to_string(),
            name: request.title.or_else(|| Some("桌面端会话".to_string())),
            project_root: request.project_root,
            provider: request.provider.unwrap_or(default_provider),
            model: request.model.unwrap_or(default_model),
            created_at: now,
            updated_at: now,
        };

        self.db.create_session(&session)?;
        self.set_active_session(session.id.clone())?;
        Ok(self.map_session(session, None))
    }

    pub fn sessions_messages(&self, session_id: String) -> Result<Vec<DesktopMessage>> {
        Ok(self
            .db
            .load_messages(&session_id)?
            .into_iter()
            .map(|message| DesktopMessage {
                images: stored_images(&message)
                    .into_iter()
                    .map(|image| DesktopImageOutput {
                        base64: image.base64,
                        media_type: image.media_type,
                    })
                    .collect(),
                id: message.id,
                role: message.role,
                content: message.content,
                reasoning: message.reasoning,
                tool_calls_json: message.tool_calls_json,
                tool_call_id: message.tool_call_id,
                metadata: message
                    .metadata_json
                    .as_deref()
                    .and_then(|json| serde_json::from_str(json).ok()),
                created_at: message.created_at.to_rfc3339(),
            })
            .collect())
    }

    pub fn sessions_delete(&self, session_id: String) -> Result<()> {
        self.db.delete_session(&session_id)?;
        Ok(())
    }

    pub fn sessions_update_llm(
        &self,
        session_id: String,
        provider: String,
        model: String,
    ) -> Result<()> {
        self.db.update_session_llm(&session_id, &provider, &model)?;
        Ok(())
    }

    pub fn runtime_state(&self) -> Result<RuntimeState> {
        let permission_mode = self
            .permission_mode
            .lock()
            .map_err(|_| anyhow::anyhow!("permission mode lock poisoned"))?
            .clone();
        let active_turns = self
            .cancel_tokens
            .lock()
            .map_err(|_| anyhow::anyhow!("cancel token lock poisoned"))?
            .len();
        Ok(RuntimeState {
            active_session_id: self
                .active_session_id
                .lock()
                .map_err(|_| anyhow::anyhow!("active session lock poisoned"))?
                .clone(),
            status: if active_turns > 0 { "running" } else { "idle" }.to_string(),
            permission_mode,
            context_percent: 0,
            tool_calls: format!("{} active", active_turns),
        })
    }

    pub fn permission_mode_set(&self, mode: String) -> Result<()> {
        let parsed = mode
            .parse::<yode_core::permission::PermissionMode>()
            .map_err(|err| anyhow::anyhow!(err))?;
        let mut active_mode = self
            .permission_mode
            .lock()
            .map_err(|_| anyhow::anyhow!("permission mode lock poisoned"))?;
        *active_mode = parsed.to_string();
        Ok(())
    }

    pub fn terminal_run(&self, request: TerminalRunRequest) -> Result<TerminalRunResponse> {
        let trimmed = request.command.trim();
        if trimmed.is_empty() {
            let cwd = self
                .terminal_session(&request.session_id)?
                .cwd
                .display()
                .to_string();
            return Ok(TerminalRunResponse {
                output: String::new(),
                cwd,
                exit_code: 0,
            });
        }

        let mut session = self.terminal_session(&request.session_id)?;
        let marker = format!("__YODE_TERMINAL_{}__", Uuid::new_v4().simple());
        let script = format!(
            "{{\n{}\n}}\n__yode_status=$?\nprintf '\\n{}STATUS:%s\\n' \"$__yode_status\"\nprintf '{}PWD:'\npwd\nprintf '{}ENV:'\nenv -0\n",
            trimmed, marker, marker, marker
        );

        let output = std::process::Command::new("sh")
            .arg("-lc")
            .arg(script)
            .current_dir(&session.cwd)
            .env_clear()
            .envs(&session.env)
            .output()
            .with_context(|| format!("failed to run terminal command '{}'", trimmed))?;

        let (stdout, cwd, env, exit_code) = parse_terminal_run_stdout(
            &output.stdout,
            &marker,
            &session.cwd,
            &session.env,
            output.status.code().unwrap_or(1),
        );
        session.cwd = cwd;
        session.env = env;
        self.terminal_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("terminal session lock poisoned"))?
            .insert(request.session_id, session.clone());

        let mut text = stdout;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.trim().is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(stderr.trim_end());
        }
        if text.is_empty() && exit_code != 0 {
            text.push_str("命令执行失败，无输出。");
        }

        Ok(TerminalRunResponse {
            output: text,
            cwd: session.cwd.display().to_string(),
            exit_code,
        })
    }

    pub fn terminal_close(&self, session_id: String) -> Result<()> {
        self.terminal_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("terminal session lock poisoned"))?
            .remove(&session_id);
        Ok(())
    }

    fn terminal_session(&self, session_id: &str) -> Result<TerminalSessionState> {
        let mut sessions = self
            .terminal_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("terminal session lock poisoned"))?;
        Ok(sessions
            .entry(session_id.to_string())
            .or_insert_with(|| TerminalSessionState {
                cwd: self.workspace_path.clone(),
                env: std::env::vars().collect(),
            })
            .clone())
    }

    pub fn turn_send_message(
        &self,
        app: AppHandle,
        request: SendMessageRequest,
    ) -> Result<TurnAccepted> {
        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        let content = request.content.trim().to_string();
        let images = request
            .images
            .into_iter()
            .filter(|image| {
                !image.base64.trim().is_empty() && image.media_type.starts_with("image/")
            })
            .map(|image| yode_llm::types::ImageData {
                base64: image.base64,
                media_type: image.media_type,
            })
            .collect::<Vec<_>>();
        if content.is_empty() && images.is_empty() {
            anyhow::bail!("message content cannot be empty");
        }

        let now = Utc::now();
        let session = if let Some(session_id) = request
            .session_id
            .as_deref()
            .filter(|id| !id.trim().is_empty())
        {
            let mut s = self
                .db
                .get_session(session_id)?
                .with_context(|| format!("session '{}' not found", session_id))?;

            let mut changed = false;
            if let Some(ref req_provider) = request.provider {
                if s.provider != *req_provider {
                    s.provider = req_provider.clone();
                    changed = true;
                }
            }
            if let Some(ref req_model) = request.model {
                if s.model != *req_model {
                    s.model = req_model.clone();
                    changed = true;
                }
            }
            if changed {
                self.db.update_session_llm(&s.id, &s.provider, &s.model)?;
            }
            s
        } else {
            let (default_provider, default_model) = self.default_llm_for_new_session(&config)?;
            let session = Session {
                id: Uuid::new_v4().to_string(),
                name: request
                    .title
                    .filter(|title| !title.trim().is_empty())
                    .or_else(|| Some(title_from_content_or_images(&content, images.len()))),
                project_root: if request.standalone.unwrap_or(false) {
                    None
                } else {
                    request
                        .project_root
                        .filter(|root| !root.trim().is_empty())
                        .or_else(|| Some(self.workspace_path.display().to_string()))
                },
                provider: request.provider.unwrap_or(default_provider),
                model: request.model.unwrap_or(default_model),
                created_at: now,
                updated_at: now,
            };
            self.db.create_session(&session)?;
            session
        };

        self.set_active_session(session.id.clone())?;
        self.db.touch_session(&session.id)?;
        let accepted_session = self.map_session(session.clone(), Some(session.id.as_str()));

        let turn_id = Uuid::new_v4().to_string();
        let session_id = session.id.clone();
        let emit_turn_id = turn_id.clone();
        let seq_base = self.seq.fetch_add(100, Ordering::SeqCst);

        let provider = self
            .provider_registry
            .lock()
            .map_err(|_| anyhow::anyhow!("registry lock poisoned"))?
            .get(&session.provider)
            .ok_or_else(|| {
                anyhow::anyhow!("Provider '{}' not found in registry", session.provider)
            })?;

        let turn_workspace_path = session
            .project_root
            .as_deref()
            .filter(|root| !root.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.workspace_path.clone());

        let mut permissions = configure_desktop_permissions(&config, &turn_workspace_path);
        if let Ok(active_mode_guard) = self.permission_mode.lock() {
            if let Ok(mode) = active_mode_guard.parse::<yode_core::permission::PermissionMode>() {
                permissions.set_mode(mode);
            }
        }
        if let Ok(rules) = self.session_permission_rules.lock() {
            if let Some(session_rules) = rules.get(&session.id) {
                permissions.add_rules(session_rules.clone());
            }
        }
        let mut context = AgentContext::resume(
            session.id.clone(),
            turn_workspace_path,
            session.provider.clone(),
            session.model.clone(),
        );
        context.project_memory_enabled = session
            .project_root
            .as_deref()
            .is_some_and(|root| !root.trim().is_empty());
        context.output_style = config.ui.output_style.clone();

        let stored_msgs = self.db.load_messages(&session.id)?;
        let restored_messages: Vec<yode_llm::types::Message> = stored_msgs
            .into_iter()
            .filter_map(stored_message_to_message)
            .collect();

        let tools = self.tool_registry.clone();
        let mcp_resource_provider = self.mcp_resource_provider.clone();
        let config = config.clone();
        let db_path_clone = self.db_path.clone();

        let (confirm_tx, confirm_rx) = unbounded_channel::<ConfirmResponse>();
        {
            let mut txs = self
                .confirm_txs
                .lock()
                .map_err(|_| anyhow::anyhow!("poisoned"))?;
            txs.insert((session_id.clone(), emit_turn_id.clone()), confirm_tx);
        }

        let (ask_user_query_tx, mut ask_user_query_rx) =
            unbounded_channel::<yode_tools::tool::UserQuery>();
        let (ask_user_answer_tx, ask_user_answer_rx) = unbounded_channel::<String>();
        {
            let mut txs = self
                .ask_user_txs
                .lock()
                .map_err(|_| anyhow::anyhow!("poisoned"))?;
            txs.insert(
                (session_id.clone(), emit_turn_id.clone()),
                ask_user_answer_tx,
            );
        }

        let cancel_token = tokio_util::sync::CancellationToken::new();
        {
            let mut tokens = self
                .cancel_tokens
                .lock()
                .map_err(|_| anyhow::anyhow!("poisoned"))?;
            tokens.insert(
                (session_id.clone(), emit_turn_id.clone()),
                cancel_token.clone(),
            );
        }

        let confirm_txs_clone = self.confirm_txs.clone();
        let ask_user_txs_clone = self.ask_user_txs.clone();
        let cancel_tokens_clone = self.cancel_tokens.clone();
        let pending_confirmations_clone = self.pending_confirmations.clone();

        std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(err) => {
                    tracing::error!("Failed to create tokio runtime: {}", err);
                    return;
                }
            };

            rt.block_on(async {
                let mut engine = AgentEngine::new(provider, tools, permissions, context);
                let db_clone = match Database::open(&db_path_clone) {
                    Ok(db) => db,
                    Err(err) => {
                        tracing::error!("Failed to open database in background thread: {}", err);
                        let desktop_event = DesktopEvent {
                            session_id: session_id.clone(),
                            turn_id: emit_turn_id.clone(),
                            seq: seq_base,
                            kind: "error".to_string(),
                            timestamp: Utc::now().to_rfc3339(),
                            payload: json!({ "body": err.to_string() }),
                        };
                        let _ = app.emit("desktop-event", desktop_event);
                        return;
                    }
                };
                engine.set_database(db_clone);
                if let Some(mcp) = mcp_resource_provider {
                    engine.set_mcp_resource_provider(mcp);
                }
                engine.set_ask_user_channels(ask_user_query_tx, ask_user_answer_rx);
                engine.set_mcp_resource_policy(yode_tools::tool::McpResourcePolicy {
                    allow: config.mcp.resource_allow.clone(),
                    deny: config.mcp.resource_deny.clone(),
                });
                engine.restore_messages(restored_messages);

                let (event_tx, mut event_rx) = unbounded_channel::<EngineEvent>();

                let session_id_str = session_id.clone();
                let turn_id_str = emit_turn_id.clone();

                let error_event_tx = event_tx.clone();
                let handle = tokio::spawn(async move {
                    if let Err(err) = engine
                        .run_turn_streaming_with_images(
                            &content,
                            images,
                            yode_core::context::QuerySource::User,
                            event_tx,
                            confirm_rx,
                            Some(cancel_token),
                        )
                        .await
                    {
                        tracing::error!("AgentEngine run_turn_streaming failed: {}", err);
                        let _ = error_event_tx.send(EngineEvent::Error(err.to_string()));
                    }
                });

                let mut seq = seq_base;
                loop {
                    let event = tokio::select! {
                        Some(query) = ask_user_query_rx.recv() => {
                            let first_question = query.questions.first();
                            let desktop_event = DesktopEvent {
                                session_id: session_id_str.clone(),
                                turn_id: turn_id_str.clone(),
                                seq,
                                kind: "ask_user".to_string(),
                                timestamp: Utc::now().to_rfc3339(),
                                payload: json!({
                                    "id": query.id,
                                    "title": first_question.map(|question| question.header.clone()).unwrap_or_else(|| "需要用户输入".to_string()),
                                    "body": first_question.map(|question| question.question.clone()).unwrap_or_else(|| "请在输入框回复。".to_string()),
                                    "query": query
                                }),
                            };
                            let _ = app.emit("desktop-event", desktop_event);
                            seq += 1;
                            continue;
                        }
                        Some(event) = event_rx.recv() => event,
                        else => break,
                    };
                    let (kind, payload) = match event {
                        EngineEvent::Thinking => {
                            ("turn_started", json!({ "title": "思考中...", "body": "" }))
                        }
                        EngineEvent::UsageUpdate(usage) => (
                            "usage_update",
                            json!({
                                "title": "用量更新",
                                "body": format!("输入 {}，输出 {}", usage.prompt_tokens, usage.completion_tokens),
                                "inputTokens": usage.prompt_tokens,
                                "outputTokens": usage.completion_tokens,
                                "status": "running"
                            }),
                        ),
                        EngineEvent::TextDelta(text) => {
                            ("assistant_text_delta", json!({ "body": text }))
                        }
                        EngineEvent::ActionNarrative(text) => (
                            "action_narrative",
                            json!({ "body": text, "status": "success" }),
                        ),
                        EngineEvent::TextComplete(text) => (
                            "assistant_text_complete",
                            json!({ "body": text, "status": "completed" }),
                        ),
                        EngineEvent::ReasoningDelta(reasoning) => {
                            ("assistant_reasoning_delta", json!({ "reasoning": reasoning }))
                        }
                        EngineEvent::ReasoningComplete(reasoning) => (
                            "assistant_reasoning_complete",
                            json!({ "reasoning": reasoning, "status": "completed" }),
                        ),
                        EngineEvent::ToolCallStart {
                            id,
                            name,
                            arguments,
                        } => (
                            "tool_started",
                            json!({
                                "id": id,
                                "tool": name,
                                "title": format!("调用工具: {}", name),
                                "body": arguments,
                                "status": "running"
                            }),
                        ),
                        EngineEvent::ToolConfirmRequired {
                            id,
                            name,
                            arguments,
                        } => {
                            if let Ok(mut pending) = pending_confirmations_clone.lock() {
                                pending.insert(
                                    (session_id_str.clone(), turn_id_str.clone()),
                                    PendingConfirmation {
                                        tool_name: name.clone(),
                                        command: extract_command_for_permission(&name, &arguments),
                                    },
                                );
                            }
                            (
                                "tool_confirm_required",
                                json!({
                                    "id": id,
                                    "tool": name,
                                    "title": format!("请求执行工具: {}", name),
                                    "body": arguments,
                                    "meta": "危险操作需要授权"
                                }),
                            )
                        }
                        EngineEvent::ToolProgress { id, name, progress } => (
                            "tool_progress",
                            json!({
                                "id": id,
                                "tool": name,
                                "title": format!("工具进度: {}", name),
                                "body": progress.message,
                                "percent": progress.percent,
                                "status": "running"
                            }),
                        ),
                        EngineEvent::ToolResult { id, name, result } => {
                            let (status, body) = if result.is_error {
                                ("blocked", result.content.clone())
                            } else {
                                ("success", result.content.clone())
                            };
                            (
                                "tool_result",
                                json!({
                                    "id": id,
                                    "tool": name,
                                    "title": format!("工具返回: {}", name),
                                    "body": body,
                                    "status": status,
                                    "errorType": result.error_type.map(|kind| format!("{:?}", kind)),
                                    "recoverable": result.recoverable,
                                    "suggestion": result.suggestion,
                                    "metadata": result.metadata
                                }),
                            )
                        }
                        EngineEvent::TurnComplete(response) => (
                            "turn_completed",
                            json!({
                                "status": "completed",
                                "body": response.message.content.unwrap_or_default(),
                                "reasoning": response.message.reasoning.unwrap_or_default(),
                                "hasToolCalls": !response.message.tool_calls.is_empty(),
                                "toolCallCount": response.message.tool_calls.len(),
                                "model": response.model,
                                "stopReason": response.stop_reason.map(|reason| format!("{:?}", reason)),
                                "inputTokens": response.usage.prompt_tokens,
                                "outputTokens": response.usage.completion_tokens,
                                "totalTokens": response.usage.total_tokens,
                                "contextPercent": 0
                            }),
                        ),
                        EngineEvent::Error(err_msg) => ("error", json!({ "body": err_msg })),
                        EngineEvent::Retrying {
                            error_message,
                            attempt,
                            max_attempts,
                            delay_secs,
                        } => (
                            "retrying",
                            json!({
                                "title": "正在重试",
                                "body": error_message,
                                "attempt": attempt,
                                "maxAttempts": max_attempts,
                                "delaySecs": delay_secs,
                                "status": "running"
                            }),
                        ),
                        EngineEvent::AskUser { id, question } => (
                            "ask_user",
                            json!({
                                "id": id,
                                "title": "需要用户输入",
                                "body": question,
                                "tool": "ask_user",
                                "meta": "等待用户回答"
                            }),
                        ),
                        EngineEvent::Done => (
                            "done",
                            json!({
                                "title": "完成",
                                "body": "本轮已完成。",
                                "status": "completed"
                            }),
                        ),
                        EngineEvent::SubAgentStart { description } => (
                            "subagent_started",
                            json!({
                                "title": "子代理启动",
                                "body": description,
                                "tool": "agent",
                                "status": "running"
                            }),
                        ),
                        EngineEvent::SubAgentComplete { result } => (
                            "subagent_completed",
                            json!({
                                "title": "子代理完成",
                                "body": result,
                                "tool": "agent",
                                "status": "success"
                            }),
                        ),
                        EngineEvent::PlanModeEntered => (
                            "plan_mode_entered",
                            json!({ "title": "计划模式", "body": "已进入计划模式。" }),
                        ),
                        EngineEvent::PlanApprovalRequired { plan_content } => (
                            "plan_approval_required",
                            json!({
                                "title": "计划需要确认",
                                "body": plan_content,
                                "tool": "plan",
                                "meta": "等待确认"
                            }),
                        ),
                        EngineEvent::PlanModeExited => (
                            "plan_mode_exited",
                            json!({ "title": "计划模式", "body": "已退出计划模式。" }),
                        ),
                        EngineEvent::ContextCompactionStarted { mode } => (
                            "context_compaction_started",
                            json!({
                                "title": "上下文压缩开始",
                                "body": mode,
                                "status": "running"
                            }),
                        ),
                        EngineEvent::ContextCompressed {
                            mode,
                            removed,
                            tool_results_truncated,
                            summary,
                            session_memory_path,
                            transcript_path,
                        } => (
                            "context_compressed",
                            json!({
                                "title": "上下文已压缩",
                                "body": summary.unwrap_or_else(|| format!("模式 {}，移除 {} 条，截断 {} 个工具结果。", mode, removed, tool_results_truncated)),
                                "mode": mode,
                                "removed": removed,
                                "toolResultsTruncated": tool_results_truncated,
                                "sessionMemoryPath": session_memory_path,
                                "transcriptPath": transcript_path
                            }),
                        ),
                        EngineEvent::CostUpdate {
                            estimated_cost,
                            input_tokens,
                            output_tokens,
                            cache_write_tokens,
                            cache_read_tokens,
                        } => (
                            "cost_update",
                            json!({
                                "title": "成本更新",
                                "body": format!("${:.4}，输入 {}，输出 {}", estimated_cost, input_tokens, output_tokens),
                                "estimatedCost": estimated_cost,
                                "inputTokens": input_tokens,
                                "outputTokens": output_tokens,
                                "cacheWriteTokens": cache_write_tokens,
                                "cacheReadTokens": cache_read_tokens
                            }),
                        ),
                        EngineEvent::BudgetExceeded { cost, limit } => (
                            "budget_exceeded",
                            json!({
                                "title": "预算已超出",
                                "body": format!("当前成本 ${:.4}，限制 ${:.4}", cost, limit),
                                "status": "blocked"
                            }),
                        ),
                        EngineEvent::SuggestionReady { suggestion } => (
                            "suggestion_ready",
                            json!({ "title": "建议", "body": suggestion }),
                        ),
                        EngineEvent::SessionMemoryUpdated {
                            path,
                            generated_summary,
                        } => (
                            "session_memory_updated",
                            json!({
                                "title": "会话记忆已更新",
                                "body": path,
                                "generatedSummary": generated_summary
                            }),
                        ),
                        EngineEvent::UpdateAvailable(version) => (
                            "update_available",
                            json!({ "title": "发现更新", "body": version }),
                        ),
                        EngineEvent::UpdateDownloading => (
                            "update_downloading",
                            json!({ "title": "正在下载更新", "body": "" }),
                        ),
                        EngineEvent::UpdateDownloaded(version) => (
                            "update_downloaded",
                            json!({ "title": "更新已下载", "body": version }),
                        ),
                    };

                    if std::env::var("YODE_ACTION_NARRATIVE_DEBUG")
                        .is_ok_and(|value| value == "1")
                        && matches!(
                            kind,
                            "assistant_text_delta"
                                | "assistant_reasoning_delta"
                                | "action_narrative"
                                | "tool_started"
                                | "assistant_text_complete"
                                | "assistant_reasoning_complete"
                                | "turn_completed"
                        )
                    {
                        let preview = payload
                            .get("body")
                            .or_else(|| payload.get("reasoning"))
                            .and_then(|value| value.as_str())
                            .unwrap_or("")
                            .chars()
                            .take(120)
                            .collect::<String>()
                            .replace('\n', "\\n");
                        eprintln!(
                            "[action-narrative-debug] turn={} kind={} preview={:?}",
                            turn_id_str, kind, preview
                        );
                    }

                    let desktop_event = DesktopEvent {
                        session_id: session_id_str.clone(),
                        turn_id: turn_id_str.clone(),
                        seq,
                        kind: kind.to_string(),
                        timestamp: Utc::now().to_rfc3339(),
                        payload,
                    };

                    let _ = app.emit("desktop-event", desktop_event);
                    seq += 1;
                }

                let _ = handle.await;

                if let Ok(mut txs) = confirm_txs_clone.lock() {
                    txs.remove(&(session_id.clone(), emit_turn_id.clone()));
                }
                if let Ok(mut txs) = ask_user_txs_clone.lock() {
                    txs.remove(&(session_id.clone(), emit_turn_id.clone()));
                }
                if let Ok(mut tokens) = cancel_tokens_clone.lock() {
                    let _: Option<tokio_util::sync::CancellationToken> =
                        tokens.remove(&(session_id.clone(), emit_turn_id.clone()));
                }
                if let Ok(mut pending) = pending_confirmations_clone.lock() {
                    pending.remove(&(session_id.clone(), emit_turn_id.clone()));
                }
            });
        });

        Ok(TurnAccepted {
            session_id: session.id,
            turn_id,
            session: accepted_session,
        })
    }

    pub fn permission_respond(
        &self,
        session_id: String,
        turn_id: String,
        allow: bool,
        always_allow: bool,
    ) -> Result<()> {
        let pending_request = self
            .pending_confirmations
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&(session_id.clone(), turn_id.clone())));

        if allow && always_allow {
            if let Some(request) = pending_request {
                let rule = PermissionRule {
                    source: RuleSource::Session,
                    behavior: RuleBehavior::Allow,
                    tool_name: request.tool_name,
                    category: None,
                    pattern: request.command,
                    description: Some("Allowed from desktop confirmation prompt".to_string()),
                };
                let mut rules = self
                    .session_permission_rules
                    .lock()
                    .map_err(|_| anyhow::anyhow!("poisoned"))?;
                rules.entry(session_id.clone()).or_default().push(rule);
            }
        }

        let tx = self
            .confirm_txs
            .lock()
            .map_err(|_| anyhow::anyhow!("poisoned"))?
            .get(&(session_id, turn_id))
            .cloned();
        if let Some(tx) = tx {
            let response = if allow && always_allow {
                ConfirmResponse::AllowAlways
            } else if allow {
                ConfirmResponse::Allow
            } else {
                ConfirmResponse::Deny
            };
            let _ = tx.send(response);
        }
        Ok(())
    }

    pub fn ask_user_respond(
        &self,
        session_id: String,
        turn_id: String,
        answer: String,
    ) -> Result<()> {
        let txs = self
            .ask_user_txs
            .lock()
            .map_err(|_| anyhow::anyhow!("poisoned"))?;
        if let Some(tx) = txs.get(&(session_id, turn_id)) {
            let _ = tx.send(answer);
        }
        Ok(())
    }

    pub fn turn_cancel(&self, session_id: String, turn_id: String) -> Result<()> {
        let mut tokens = self
            .cancel_tokens
            .lock()
            .map_err(|_| anyhow::anyhow!("poisoned"))?;
        if let Some(token) = tokens.remove(&(session_id, turn_id)) {
            let token: tokio_util::sync::CancellationToken = token;
            token.cancel();
        }
        Ok(())
    }

    fn set_active_session(&self, session_id: String) -> Result<()> {
        *self
            .active_session_id
            .lock()
            .map_err(|_| anyhow::anyhow!("active session lock poisoned"))? = Some(session_id);
        Ok(())
    }

    fn map_session(&self, session: Session, active_session_id: Option<&str>) -> DesktopSession {
        DesktopSession {
            id: session.id.clone(),
            title: session
                .name
                .clone()
                .unwrap_or_else(|| session.id.chars().take(8).collect()),
            project: session
                .project_root
                .as_deref()
                .and_then(project_label_from_root),
            project_root: session.project_root.clone(),
            provider: session.provider,
            model: session.model,
            updated_at: relative_time(session.updated_at),
            active: active_session_id == Some(session.id.as_str()),
        }
    }

    fn default_llm_for_new_session(&self, config: &Config) -> Result<(String, String)> {
        if let Some(session) = self.db.list_sessions(1)?.into_iter().next() {
            if !session.provider.trim().is_empty() && !session.model.trim().is_empty() {
                return Ok((session.provider, session.model));
            }
        }
        Ok((
            config.llm.default_provider.clone(),
            config.llm.default_model.clone(),
        ))
    }

    pub fn config_get_providers(&self) -> Result<Vec<DesktopProvider>> {
        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        let mut providers = Vec::new();
        for (id, p) in &config.llm.providers {
            let name = match id.as_str() {
                "anthropic" => "Anthropic Claude".to_string(),
                "openai" => "OpenAI".to_string(),
                "google" | "gemini" => "Google Gemini".to_string(),
                "deepseek" => "DeepSeek (深度求索)".to_string(),
                "ollama" => "Ollama (本地运行)".to_string(),
                _ => id.to_uppercase(),
            };
            providers.push(DesktopProvider {
                id: id.clone(),
                name,
                format: p.format.clone(),
                enabled: p.enabled.unwrap_or(true),
                api_key: p.api_key.clone().unwrap_or_default(),
                base_url: p.base_url.clone().unwrap_or_default(),
                models: p.models.clone(),
                gradient: p.gradient.clone(),
            });
        }
        let order = [
            "openai",
            "anthropic",
            "gemini",
            "google",
            "deepseek",
            "ollama",
        ];
        providers.sort_by_key(|p| order.iter().position(|&x| x == p.id).unwrap_or(99));
        Ok(providers)
    }

    pub fn config_get_default_llm(&self) -> Result<DefaultLlm> {
        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        Ok(DefaultLlm {
            provider: config.llm.default_provider.clone(),
            model: config.llm.default_model.clone(),
        })
    }

    pub fn config_set_default_llm(&self, provider: String, model: String) -> Result<DefaultLlm> {
        let provider = provider.trim().to_string();
        let model = model.trim().to_string();
        if provider.is_empty() || model.is_empty() {
            anyhow::bail!("provider and model cannot be empty");
        }
        let mut config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        if !config.llm.providers.contains_key(&provider) {
            anyhow::bail!("Provider '{}' not found", provider);
        }
        config.llm.default_provider = provider;
        config.llm.default_model = model;
        config.save()?;
        Ok(DefaultLlm {
            provider: config.llm.default_provider.clone(),
            model: config.llm.default_model.clone(),
        })
    }

    pub fn config_save_providers(&self, providers: Vec<DesktopProvider>) -> Result<()> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        let mut new_providers = std::collections::HashMap::new();
        for p in providers {
            new_providers.insert(
                p.id,
                yode_core::config::ProviderConfig {
                    format: p.format,
                    base_url: if p.base_url.is_empty() {
                        None
                    } else {
                        Some(p.base_url)
                    },
                    api_key: if p.api_key.is_empty() {
                        None
                    } else {
                        Some(p.api_key)
                    },
                    models: p.models,
                    enabled: Some(p.enabled),
                    gradient: p.gradient,
                },
            );
        }
        if !new_providers.contains_key(&config.llm.default_provider) {
            if let Some((provider, config_provider)) = new_providers
                .iter()
                .find(|(_, provider)| provider.enabled.unwrap_or(true))
                .or_else(|| new_providers.iter().next())
            {
                config.llm.default_provider = provider.clone();
                config.llm.default_model = config_provider
                    .models
                    .first()
                    .cloned()
                    .unwrap_or_else(|| config.llm.default_model.clone());
            }
        }
        config.llm.providers = new_providers;
        config.save()?;

        let new_registry = bootstrap_providers(&config);
        let mut reg_guard = self
            .provider_registry
            .lock()
            .map_err(|_| anyhow::anyhow!("registry lock poisoned"))?;
        *reg_guard = new_registry;

        Ok(())
    }

    pub async fn config_test_provider(&self, p: DesktopProvider) -> Result<()> {
        let api_key = resolve_provider_api_key(&p.id, &p.format, p.api_key.trim());
        let base_url = resolve_provider_base_url(&p.id, &p.format, p.base_url.trim());
        let provider: Arc<dyn yode_llm::provider::LlmProvider> = match p.format.as_str() {
            "anthropic" => Arc::new(yode_llm::providers::anthropic::AnthropicProvider::new(
                &p.id, &api_key, &base_url,
            )),
            "gemini" => {
                let mut provider = yode_llm::providers::gemini::GeminiProvider::new(&api_key);
                if base_url != "https://generativelanguage.googleapis.com/v1beta" {
                    provider = provider.with_base_url(&base_url);
                }
                Arc::new(provider)
            }
            _ => Arc::new(yode_llm::providers::openai::OpenAiProvider::new(
                &p.id, &api_key, &base_url,
            )),
        };

        let _models = provider.list_models().await?;
        Ok(())
    }
}

fn resolve_provider_api_key(id: &str, format: &str, configured: &str) -> String {
    if !configured.is_empty() {
        return configured.to_string();
    }

    let env_prefix = id.to_uppercase().replace('-', "_");
    let mut candidates = vec![format!("{}_API_KEY", env_prefix)];
    candidates.extend(match (id, format) {
        ("anthropic", _) | (_, "anthropic") => vec![
            "ANTHROPIC_API_KEY".to_string(),
            "ANTHROPIC_AUTH_TOKEN".to_string(),
        ],
        ("gemini", _) | ("google", _) | (_, "gemini") => {
            vec!["GOOGLE_API_KEY".to_string(), "GEMINI_API_KEY".to_string()]
        }
        ("deepseek", _) => vec!["DEEPSEEK_API_KEY".to_string()],
        ("openai", _) => vec!["OPENAI_API_KEY".to_string()],
        _ => Vec::new(),
    });

    candidates
        .into_iter()
        .find_map(|key| std::env::var(key).ok())
        .unwrap_or_default()
}

fn resolve_provider_base_url(id: &str, format: &str, configured: &str) -> String {
    let env_prefix = id.to_uppercase().replace('-', "_");
    let override_base = format!("{}_BASE_URL", env_prefix);
    if let Ok(url) = std::env::var(override_base) {
        return url;
    }
    if !configured.is_empty() {
        return configured.to_string();
    }
    match format {
        "anthropic" => "https://api.anthropic.com".to_string(),
        "gemini" => "https://generativelanguage.googleapis.com/v1beta".to_string(),
        _ => "https://api.openai.com/v1".to_string(),
    }
}

fn bootstrap_providers(config: &Config) -> Arc<ProviderRegistry> {
    let registry = ProviderRegistry::new();
    for (name, p_config) in &config.llm.providers {
        let env_prefix = name.to_uppercase().replace('-', "_");
        let override_key = format!("{}_API_KEY", env_prefix);
        let api_key = if let Ok(key) = std::env::var(&override_key) {
            key
        } else if let Some(key) = p_config.api_key.clone() {
            key
        } else {
            let fallback_keys = match name.as_str() {
                "anthropic" => vec!["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"],
                "openai" => vec!["OPENAI_API_KEY"],
                "gemini" | "google" => vec!["GOOGLE_API_KEY", "GEMINI_API_KEY"],
                "deepseek" => vec!["DEEPSEEK_API_KEY"],
                _ => vec![],
            };
            let mut resolved = String::new();
            for key in fallback_keys {
                if let Ok(val) = std::env::var(key) {
                    resolved = val;
                    break;
                }
            }
            resolved
        };

        let override_base = format!("{}_BASE_URL", env_prefix);
        let base_url = if let Ok(url) = std::env::var(&override_base) {
            url
        } else if let Some(url) = p_config.base_url.clone() {
            url
        } else {
            match p_config.format.as_str() {
                "anthropic" => "https://api.anthropic.com".to_string(),
                "gemini" => "https://generativelanguage.googleapis.com/v1beta".to_string(),
                _ => "https://api.openai.com/v1".to_string(),
            }
        };

        match p_config.format.as_str() {
            "anthropic" => {
                registry.register(Arc::new(
                    yode_llm::providers::anthropic::AnthropicProvider::new(
                        name, &api_key, &base_url,
                    ),
                ));
            }
            "gemini" => {
                let mut provider = yode_llm::providers::gemini::GeminiProvider::new(&api_key);
                if base_url != "https://generativelanguage.googleapis.com/v1beta" {
                    provider = provider.with_base_url(&base_url);
                }
                registry.register(Arc::new(provider));
            }
            _ => {
                registry.register(Arc::new(yode_llm::providers::openai::OpenAiProvider::new(
                    name, &api_key, &base_url,
                )));
            }
        }
    }
    Arc::new(registry)
}

fn setup_desktop_tooling(
    config: &Config,
    workdir: &std::path::Path,
) -> (Arc<ToolRegistry>, Option<Arc<dyn McpResourceProvider>>) {
    let tool_registry = ToolRegistry::new();
    yode_tools::builtin::register_builtin_tools(&tool_registry);

    let mut mcp_clients = Vec::new();
    for (name, server_config) in &config.mcp.servers {
        if server_config.disabled {
            continue;
        }
        let mcp_config = yode_mcp::McpServerConfig {
            disabled: server_config.disabled,
            transport: match server_config.transport {
                yode_core::config::McpTransportConfig::Stdio => yode_mcp::McpTransportConfig::Stdio,
                yode_core::config::McpTransportConfig::Sse => yode_mcp::McpTransportConfig::Sse,
                yode_core::config::McpTransportConfig::Http => yode_mcp::McpTransportConfig::Http,
                yode_core::config::McpTransportConfig::Websocket => {
                    yode_mcp::McpTransportConfig::Websocket
                }
            },
            command: server_config.command.clone(),
            args: server_config.args.clone(),
            env: server_config.env.clone(),
            url: server_config.url.clone(),
            auth: server_config
                .auth
                .as_ref()
                .map(|auth| yode_mcp::McpAuthConfig {
                    oauth: auth.oauth.as_ref().map(|oauth| yode_mcp::McpOAuthConfig {
                        client_id: oauth.client_id.clone(),
                        authorization_url: oauth.authorization_url.clone(),
                        token_url: oauth.token_url.clone(),
                        scopes: oauth.scopes.clone(),
                    }),
                    bearer_token_env: auth.bearer_token_env.clone(),
                }),
        };

        if let Ok(client) = tauri::async_runtime::block_on(async {
            yode_mcp::McpClient::connect(name, &mcp_config).await
        }) {
            if let Ok(wrappers) =
                tauri::async_runtime::block_on(async { client.discover_wrapped_tools().await })
            {
                for wrapper in wrappers {
                    tool_registry.register(wrapper);
                }
            }
            mcp_clients.push(client);
        }
    }

    let skill_paths = yode_core::skills::SkillRegistry::default_paths(workdir);
    let skill_registry = yode_core::skills::SkillRegistry::discover(&skill_paths);
    use yode_tools::builtin::skill::{SkillContextMode, SkillEntry, SkillStore};
    let mut store = SkillStore::new();
    for skill in skill_registry.list() {
        let context = match skill.metadata.context {
            yode_core::skills::SkillContextMode::Inline => SkillContextMode::Inline,
            yode_core::skills::SkillContextMode::Fork => SkillContextMode::Fork,
        };
        store.add_entry(SkillEntry {
            name: skill.name.clone(),
            description: skill.description.clone(),
            content: skill.content.clone(),
            allowed_tools: skill.metadata.allowed_tools.clone(),
            paths: skill.metadata.paths.clone(),
            trigger_examples: skill.metadata.trigger_examples.clone(),
            context,
            model: skill.metadata.model.clone(),
            effort: skill.metadata.effort.clone(),
        });
    }
    let store = Arc::new(tokio::sync::Mutex::new(store));
    yode_tools::builtin::register_skill_tool(&tool_registry, store);

    let mcp_resource_provider = if !mcp_clients.is_empty() {
        Some(
            Arc::new(yode_mcp::McpClientResourceProvider::new(mcp_clients))
                as Arc<dyn McpResourceProvider>,
        )
    } else {
        None
    };

    (Arc::new(tool_registry), mcp_resource_provider)
}

fn configure_desktop_permissions(config: &Config, _workdir: &std::path::Path) -> PermissionManager {
    let mut permissions =
        PermissionManager::from_confirmation_list(config.tools.require_confirmation.clone());
    if let Some(mode_str) = &config.permissions.default_mode {
        if let Ok(mode) = mode_str.parse::<yode_core::permission::PermissionMode>() {
            permissions.set_mode(mode);
        }
    }
    for rule in &config.permissions.always_allow {
        permissions.add_rule(PermissionRule {
            source: RuleSource::UserConfig,
            behavior: RuleBehavior::Allow,
            tool_name: rule.tool.clone(),
            category: rule.category.clone(),
            pattern: rule.pattern.clone(),
            description: rule.description.clone(),
        });
    }
    for rule in &config.permissions.always_deny {
        permissions.add_rule(PermissionRule {
            source: RuleSource::UserConfig,
            behavior: RuleBehavior::Deny,
            tool_name: rule.tool.clone(),
            category: rule.category.clone(),
            pattern: rule.pattern.clone(),
            description: rule.description.clone(),
        });
    }
    permissions
}

fn extract_command_for_permission(tool_name: &str, arguments: &str) -> Option<String> {
    if tool_name != "bash" {
        return None;
    }

    serde_json::from_str::<serde_json::Value>(arguments)
        .ok()
        .and_then(|value| {
            value
                .get("command")
                .and_then(|command| command.as_str())
                .map(str::to_string)
        })
        .or_else(|| Some(arguments.to_string()))
}

fn project_label_from_root(project_root: &str) -> Option<String> {
    let trimmed = project_root.trim();
    if trimmed.is_empty() {
        return None;
    }

    PathBuf::from(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
}

fn title_from_content(content: &str) -> String {
    let title = content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(28)
        .collect::<String>();

    if title.is_empty() {
        "新对话".to_string()
    } else {
        title
    }
}

fn title_from_content_or_images(content: &str, image_count: usize) -> String {
    if !content.trim().is_empty() {
        return title_from_content(content);
    }
    if image_count > 1 {
        format!("{} 张图片", image_count)
    } else {
        "图片".to_string()
    }
}

fn stored_message_to_message(message: StoredMessage) -> Option<yode_llm::types::Message> {
    let role = match message.role.as_str() {
        "user" => yode_llm::types::Role::User,
        "assistant" => yode_llm::types::Role::Assistant,
        "tool" => yode_llm::types::Role::Tool,
        "system" => yode_llm::types::Role::System,
        _ => return None,
    };
    let tool_calls: Vec<yode_llm::types::ToolCall> = message
        .tool_calls_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();
    let mut blocks = Vec::new();
    if let Some(reasoning) = &message.reasoning {
        blocks.push(yode_llm::types::ContentBlock::Thinking {
            thinking: reasoning.clone(),
            signature: None,
        });
    }
    if let Some(content) = &message.content {
        blocks.push(yode_llm::types::ContentBlock::Text {
            text: content.clone(),
        });
    }

    let images = stored_images(&message);

    Some(
        yode_llm::types::Message {
            role,
            content: message.content,
            content_blocks: blocks,
            reasoning: message.reasoning,
            tool_calls,
            tool_call_id: message.tool_call_id,
            images,
        }
        .normalized(),
    )
}

fn stored_images(message: &StoredMessage) -> Vec<yode_llm::types::ImageData> {
    message
        .images_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default()
}

fn resolve_desktop_workspace_path() -> PathBuf {
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    find_workspace_root(&current_dir).unwrap_or(current_dir)
}

fn find_workspace_root(start: &std::path::Path) -> Option<PathBuf> {
    for ancestor in start.ancestors() {
        if ancestor.join(".git").is_dir() || is_cargo_workspace_root(ancestor) {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

fn is_cargo_workspace_root(path: &std::path::Path) -> bool {
    std::fs::read_to_string(path.join("Cargo.toml"))
        .map(|content| content.contains("[workspace]"))
        .unwrap_or(false)
}

fn parse_terminal_run_stdout(
    stdout: &[u8],
    marker: &str,
    fallback_cwd: &std::path::Path,
    fallback_env: &HashMap<String, String>,
    fallback_exit_code: i32,
) -> (String, PathBuf, HashMap<String, String>, i32) {
    let status_marker = format!("\n{}STATUS:", marker).into_bytes();
    let Some(status_start) = find_bytes(stdout, &status_marker) else {
        return (
            String::from_utf8_lossy(stdout).trim_end().to_string(),
            fallback_cwd.to_path_buf(),
            fallback_env.clone(),
            fallback_exit_code,
        );
    };

    let visible_stdout = String::from_utf8_lossy(&stdout[..status_start])
        .trim_end_matches('\n')
        .to_string();
    let status_value_start = status_start + status_marker.len();
    let status_end = stdout[status_value_start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map(|offset| status_value_start + offset)
        .unwrap_or(stdout.len());
    let exit_code = String::from_utf8_lossy(&stdout[status_value_start..status_end])
        .trim()
        .parse::<i32>()
        .unwrap_or(fallback_exit_code);

    let pwd_marker = format!("{}PWD:", marker).into_bytes();
    let env_marker = format!("{}ENV:", marker).into_bytes();
    let pwd_start =
        find_bytes_from(stdout, &pwd_marker, status_end).map(|idx| idx + pwd_marker.len());
    let env_start = find_bytes_from(stdout, &env_marker, status_end);

    let cwd = pwd_start
        .and_then(|start| {
            let end = stdout[start..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map(|offset| start + offset)
                .unwrap_or(stdout.len());
            let path = String::from_utf8_lossy(&stdout[start..end])
                .trim()
                .to_string();
            if path.is_empty() {
                None
            } else {
                Some(PathBuf::from(path))
            }
        })
        .unwrap_or_else(|| fallback_cwd.to_path_buf());

    let env = env_start
        .map(|start| parse_null_delimited_env(&stdout[start + env_marker.len()..]))
        .filter(|env| !env.is_empty())
        .unwrap_or_else(|| fallback_env.clone());

    (visible_stdout, cwd, env, exit_code)
}

fn parse_null_delimited_env(bytes: &[u8]) -> HashMap<String, String> {
    bytes
        .split(|byte| *byte == 0)
        .filter_map(|entry| {
            if entry.is_empty() {
                return None;
            }
            let eq = entry.iter().position(|byte| *byte == b'=')?;
            let key = String::from_utf8_lossy(&entry[..eq]).to_string();
            let value = String::from_utf8_lossy(&entry[eq + 1..]).to_string();
            Some((key, value))
        })
        .collect()
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    find_bytes_from(haystack, needle, 0)
}

fn find_bytes_from(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() || start >= haystack.len() {
        return None;
    }
    haystack[start..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| start + offset)
}

fn relative_time(updated_at: DateTime<Utc>) -> String {
    let local_time = updated_at.with_timezone(&chrono::Local);
    local_time.format("%m月%d日 %H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_root_detection_climbs_out_of_src_tauri() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap();
        let src_tauri = root.join("apps/yode-desktop/src-tauri");

        assert_eq!(
            find_workspace_root(&src_tauri).as_deref(),
            Some(root.as_path())
        );
    }

    #[test]
    fn terminal_stdout_parser_extracts_runtime_state() {
        let marker = "__YODE_TERMINAL_TEST__";
        let stdout = b"hello\n__YODE_TERMINAL_TEST__STATUS:7\n__YODE_TERMINAL_TEST__PWD:/tmp/project\n__YODE_TERMINAL_TEST__ENV:FOO=bar\0PWD=/tmp/project\0";
        let fallback_env = HashMap::from([("FOO".to_string(), "old".to_string())]);

        let (visible, cwd, env, exit_code) = parse_terminal_run_stdout(
            stdout,
            marker,
            std::path::Path::new("/tmp"),
            &fallback_env,
            1,
        );

        assert_eq!(visible, "hello");
        assert_eq!(cwd, PathBuf::from("/tmp/project"));
        assert_eq!(env.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(exit_code, 7);
    }

    #[test]
    fn terminal_stdout_parser_falls_back_without_marker() {
        let fallback_env = HashMap::from([("FOO".to_string(), "old".to_string())]);

        let (visible, cwd, env, exit_code) = parse_terminal_run_stdout(
            b"plain output\n",
            "__YODE_TERMINAL_TEST__",
            std::path::Path::new("/tmp"),
            &fallback_env,
            2,
        );

        assert_eq!(visible, "plain output");
        assert_eq!(cwd, PathBuf::from("/tmp"));
        assert_eq!(env.get("FOO"), Some(&"old".to_string()));
        assert_eq!(exit_code, 2);
    }
}
