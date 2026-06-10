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
use uuid::Uuid;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use yode_core::config::Config;
use yode_core::db::{Database, StoredMessage};
use yode_core::session::Session;
use yode_core::context::AgentContext;
use yode_core::engine::{AgentEngine, ConfirmResponse, EngineEvent};
use yode_core::permission::{PermissionManager, PermissionRule, RuleBehavior, RuleSource};
use yode_llm::registry::ProviderRegistry;
use yode_tools::registry::ToolRegistry;
use yode_tools::tool::McpResourceProvider;

use crate::protocol::{
    Bootstrap, CreateSessionRequest, DesktopEvent, DesktopSession, RuntimeState,
    SendMessageRequest, TurnAccepted,
};

pub struct DesktopRuntime {
    config: Config,
    db: Database,
    db_path: PathBuf,
    workspace_path: PathBuf,
    provider_registry: Arc<ProviderRegistry>,
    tool_registry: Arc<ToolRegistry>,
    mcp_resource_provider: Option<Arc<dyn McpResourceProvider>>,
    active_session_id: Mutex<Option<String>>,
    permission_mode: Mutex<String>,
    seq: AtomicU64,
    confirm_txs: Arc<Mutex<HashMap<(String, String), UnboundedSender<ConfirmResponse>>>>,
    cancel_tokens: Arc<Mutex<HashMap<(String, String), tokio_util::sync::CancellationToken>>>,
    pending_confirmations: Arc<Mutex<HashMap<(String, String), PendingConfirmation>>>,
    session_permission_rules: Arc<Mutex<HashMap<String, Vec<PermissionRule>>>>,
}

#[derive(Debug, Clone)]
struct PendingConfirmation {
    tool_name: String,
    command: Option<String>,
}

impl DesktopRuntime {
    pub fn new() -> Result<Self> {
        let workspace_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let db_path = dirs::home_dir()
            .unwrap_or_else(|| workspace_path.clone())
            .join(".yode")
            .join("sessions.db");

        let config = Config::load().unwrap_or_else(|_| {
            Config::load_from(None).expect("failed to load default config")
        });

        let provider_registry = bootstrap_providers(&config);
        let (tool_registry, mcp_resource_provider) = setup_desktop_tooling(&config, &workspace_path);

        let default_mode = config.permissions.default_mode.clone().unwrap_or_else(|| "Default".to_string());

        Ok(Self {
            config,
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
            cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
            pending_confirmations: Arc::new(Mutex::new(HashMap::new())),
            session_permission_rules: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn bootstrap(&self) -> Result<Bootstrap> {
        let sessions = self.sessions_list()?;
        let permission_mode = self
            .permission_mode
            .lock()
            .map_err(|_| anyhow::anyhow!("permission mode lock poisoned"))?
            .clone();
        Ok(Bootstrap {
            app_version: env!("CARGO_PKG_VERSION"),
            workspace_path: self.workspace_path.display().to_string(),
            provider: self.config.llm.default_provider.clone(),
            model: self.config.llm.default_model.clone(),
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
        let session = Session {
            id: Uuid::new_v4().to_string(),
            name: request.title.or_else(|| Some("桌面端会话".to_string())),
            project_root: request.project_root,
            provider: request.provider.unwrap_or_else(|| self.config.llm.default_provider.clone()),
            model: request.model.unwrap_or_else(|| self.config.llm.default_model.clone()),
            created_at: now,
            updated_at: now,
        };

        self.db.create_session(&session)?;
        self.set_active_session(session.id.clone())?;
        Ok(self.map_session(session, None))
    }

    pub fn runtime_state(&self) -> Result<RuntimeState> {
        let permission_mode = self
            .permission_mode
            .lock()
            .map_err(|_| anyhow::anyhow!("permission mode lock poisoned"))?
            .clone();
        Ok(RuntimeState {
            active_session_id: self
                .active_session_id
                .lock()
                .map_err(|_| anyhow::anyhow!("active session lock poisoned"))?
                .clone(),
            status: "idle".to_string(),
            permission_mode,
            context_percent: 31,
            tool_calls: "0 / 0".to_string(),
        })
    }

    pub fn permission_mode_set(&self, mode: String) -> Result<()> {
        let mut active_mode = self
            .permission_mode
            .lock()
            .map_err(|_| anyhow::anyhow!("permission mode lock poisoned"))?;
        *active_mode = mode;
        Ok(())
    }

    pub fn turn_send_message(
        &self,
        app: AppHandle,
        request: SendMessageRequest,
    ) -> Result<TurnAccepted> {
        let content = request.content.trim().to_string();
        if content.is_empty() {
            anyhow::bail!("message content cannot be empty");
        }

        let session = self
            .db
            .get_session(&request.session_id)?
            .with_context(|| format!("session '{}' not found", request.session_id))?;

        self.set_active_session(session.id.clone())?;
        self.db
            .save_message(&session.id, "user", Some(&content), None, None, None)?;
        self.db.touch_session(&session.id)?;

        let turn_id = Uuid::new_v4().to_string();
        let session_id = session.id.clone();
        let emit_turn_id = turn_id.clone();
        let seq_base = self.seq.fetch_add(100, Ordering::SeqCst);

        let provider = self.provider_registry.get(&session.provider)
            .ok_or_else(|| anyhow::anyhow!("Provider '{}' not found in registry", session.provider))?;

        let mut permissions = configure_desktop_permissions(&self.config, &self.workspace_path);
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
            self.workspace_path.clone(),
            session.provider.clone(),
            session.model.clone(),
        );
        context.output_style = self.config.ui.output_style.clone();

        let stored_msgs = self.db.load_messages(&session.id)?;
        let restored_messages: Vec<yode_llm::types::Message> = stored_msgs
            .into_iter()
            .filter_map(stored_message_to_message)
            .collect();

        let tools = self.tool_registry.clone();
        let mcp_resource_provider = self.mcp_resource_provider.clone();
        let config = self.config.clone();
        let db_path_clone = self.db_path.clone();

        let (confirm_tx, confirm_rx) = unbounded_channel::<ConfirmResponse>();
        {
            let mut txs = self.confirm_txs.lock().map_err(|_| anyhow::anyhow!("poisoned"))?;
            txs.insert((session_id.clone(), emit_turn_id.clone()), confirm_tx);
        }

        let cancel_token = tokio_util::sync::CancellationToken::new();
        {
            let mut tokens = self.cancel_tokens.lock().map_err(|_| anyhow::anyhow!("poisoned"))?;
            tokens.insert((session_id.clone(), emit_turn_id.clone()), cancel_token.clone());
        }

        let confirm_txs_clone = self.confirm_txs.clone();
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
                        return;
                    }
                };
                engine.set_database(db_clone);
                if let Some(mcp) = mcp_resource_provider {
                    engine.set_mcp_resource_provider(mcp);
                }
                engine.set_mcp_resource_policy(yode_tools::tool::McpResourcePolicy {
                    allow: config.mcp.resource_allow.clone(),
                    deny: config.mcp.resource_deny.clone(),
                });
                engine.restore_messages(restored_messages);

                let (event_tx, mut event_rx) = unbounded_channel::<EngineEvent>();

                let session_id_str = session_id.clone();
                let turn_id_str = emit_turn_id.clone();
                
                let handle = tokio::spawn(async move {
                    if let Err(err) = engine.run_turn_streaming(
                        &content,
                        yode_core::context::QuerySource::User,
                        event_tx,
                        confirm_rx,
                        Some(cancel_token),
                    ).await {
                        tracing::error!("AgentEngine run_turn_streaming failed: {}", err);
                    }
                });

                let mut seq = seq_base;
                while let Some(event) = event_rx.recv().await {
                    let (kind, payload) = match event {
                        EngineEvent::Thinking => {
                            ("turn_started", json!({ "title": "思考中...", "body": "" }))
                        }
                        EngineEvent::TextDelta(text) => {
                            ("assistant_text_delta", json!({ "body": text }))
                        }
                        EngineEvent::TextComplete(text) => {
                            ("assistant_text_complete", json!({ "body": text, "status": "completed" }))
                        }
                        EngineEvent::ReasoningDelta(reasoning) => {
                            ("assistant_reasoning_delta", json!({ "body": reasoning }))
                        }
                        EngineEvent::ReasoningComplete(reasoning) => {
                            ("assistant_reasoning_complete", json!({ "body": reasoning, "status": "completed" }))
                        }
                        EngineEvent::ToolCallStart { id, name, arguments } => {
                            ("tool_started", json!({
                                "id": id,
                                "tool": name,
                                "title": format!("调用工具: {}", name),
                                "body": arguments,
                                "status": "running"
                            }))
                        }
                        EngineEvent::ToolConfirmRequired { id, name, arguments } => {
                            if let Ok(mut pending) = pending_confirmations_clone.lock() {
                                pending.insert((session_id_str.clone(), turn_id_str.clone()), PendingConfirmation {
                                    tool_name: name.clone(),
                                    command: extract_command_for_permission(&name, &arguments),
                                });
                            }
                            ("tool_confirm_required", json!({
                                "id": id,
                                "tool": name,
                                "title": format!("请求执行工具: {}", name),
                                "body": arguments,
                                "meta": "危险操作需要授权"
                            }))
                        }
                        EngineEvent::ToolProgress { id, name, progress } => {
                            ("tool_progress", json!({
                                "id": id,
                                "tool": name,
                                "title": format!("工具进度: {}", name),
                                "body": progress.message
                            }))
                        }
                        EngineEvent::ToolResult { id, name, result } => {
                            let (status, body) = if result.is_error {
                                ("blocked", format!("错误: {:?}", result))
                            } else {
                                ("success", format!("{:?}", result))
                            };
                            ("tool_result", json!({
                                "id": id,
                                "tool": name,
                                "title": format!("工具返回: {}", name),
                                "body": body,
                                "status": status
                            }))
                        }
                        EngineEvent::TurnComplete(response) => {
                            ("turn_completed", json!({
                                "status": "completed",
                                "toolCalls": format!("{}", response.usage.completion_tokens),
                                "contextPercent": 0
                            }))
                        }
                        EngineEvent::Error(err_msg) => {
                            ("error", json!({ "body": err_msg }))
                        }
                        _ => continue,
                    };

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
                if let Ok(mut tokens) = cancel_tokens_clone.lock() {
                    let _: Option<tokio_util::sync::CancellationToken> = tokens.remove(&(session_id.clone(), emit_turn_id.clone()));
                }
                if let Ok(mut pending) = pending_confirmations_clone.lock() {
                    pending.remove(&(session_id.clone(), emit_turn_id.clone()));
                }
            });
        });

        Ok(TurnAccepted {
            session_id: session.id,
            turn_id,
        })
    }

    pub fn permission_respond(&self, session_id: String, turn_id: String, allow: bool, always_allow: bool) -> Result<()> {
        if allow && always_allow {
            if let Ok(mut pending) = self.pending_confirmations.lock() {
                if let Some(request) = pending.remove(&(session_id.clone(), turn_id.clone())) {
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
        }

        let mut txs = self.confirm_txs.lock().map_err(|_| anyhow::anyhow!("poisoned"))?;
        if let Some(tx) = txs.remove(&(session_id, turn_id)) {
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

    pub fn turn_cancel(&self, session_id: String, turn_id: String) -> Result<()> {
        let mut tokens = self.cancel_tokens.lock().map_err(|_| anyhow::anyhow!("poisoned"))?;
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
            provider: session.provider,
            model: session.model,
            updated_at: relative_time(session.updated_at),
            active: active_session_id == Some(session.id.as_str()),
        }
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
                registry.register(Arc::new(yode_llm::providers::anthropic::AnthropicProvider::new(name, &api_key, &base_url)));
            }
            "gemini" => {
                let mut provider = yode_llm::providers::gemini::GeminiProvider::new(&api_key);
                if base_url != "https://generativelanguage.googleapis.com/v1beta" {
                    provider = provider.with_base_url(&base_url);
                }
                registry.register(Arc::new(provider));
            }
            _ => {
                registry.register(Arc::new(yode_llm::providers::openai::OpenAiProvider::new(name, &api_key, &base_url)));
            }
        }
    }
    Arc::new(registry)
}

fn setup_desktop_tooling(config: &Config, workdir: &std::path::Path) -> (Arc<ToolRegistry>, Option<Arc<dyn McpResourceProvider>>) {
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
                yode_core::config::McpTransportConfig::Websocket => yode_mcp::McpTransportConfig::Websocket,
            },
            command: server_config.command.clone(),
            args: server_config.args.clone(),
            env: server_config.env.clone(),
            url: server_config.url.clone(),
            auth: server_config.auth.as_ref().map(|auth| yode_mcp::McpAuthConfig {
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
            if let Ok(wrappers) = tauri::async_runtime::block_on(async {
                client.discover_wrapped_tools().await
            }) {
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
        Some(Arc::new(yode_mcp::McpClientResourceProvider::new(mcp_clients)) as Arc<dyn McpResourceProvider>)
    } else {
        None
    };

    (Arc::new(tool_registry), mcp_resource_provider)
}

fn configure_desktop_permissions(config: &Config, _workdir: &std::path::Path) -> PermissionManager {
    let mut permissions = PermissionManager::from_confirmation_list(config.tools.require_confirmation.clone());
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

    Some(
        yode_llm::types::Message {
            role,
            content: message.content,
            content_blocks: blocks,
            reasoning: message.reasoning,
            tool_calls,
            tool_call_id: message.tool_call_id,
            images: Vec::new(),
        }
        .normalized(),
    )
}

fn relative_time(updated_at: DateTime<Utc>) -> String {
    let local_time = updated_at.with_timezone(&chrono::Local);
    local_time.format("%m月%d日 %H:%M").to_string()
}
