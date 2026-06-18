use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::process::Child;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::json;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use uuid::Uuid;

use yode_core::config::Config;
use yode_core::context::AgentContext;
use yode_core::db::Database;
use yode_core::engine::{AgentEngine, ConfirmResponse, EngineEvent};
use yode_core::permission::{PermissionManager, PermissionRule, RuleBehavior, RuleSource};
use yode_core::session::Session;
use yode_llm::registry::ProviderRegistry;
use yode_tools::registry::ToolRegistry;
use yode_tools::tool::McpResourceProvider;

use crate::browser_settings::{apply_browser_settings_env, browser_settings_from_desktop_settings};
use crate::desktop_settings_store::read_desktop_settings;
use crate::git_settings::{apply_git_settings_env, git_settings_from_desktop_settings};
use crate::hook_settings::build_desktop_hook_manager;
use crate::license_notices::read_license_notices;
use crate::protocol::{
    Bootstrap, DesktopActionResult, DesktopEvent, DesktopWorktree, GeneralSettings,
    ImportAiSessionsResult, LicenseNotice, RuntimeState, SendMessageRequest, TurnAccepted,
};
use crate::session_helpers::{stored_message_to_message, title_from_content_or_images};
use crate::session_import::{collect_import_files, import_one_ai_session};
use crate::worktree::{
    current_git_branch, delete_worktree, list_git_worktrees, prune_idle_worktrees,
};

mod configuration_runtime;
mod mcp_runtime;
mod personalization_runtime;
mod provider_runtime;
mod session_runtime;
mod settings_runtime;
mod terminal_runtime;

use self::configuration_runtime::load_desktop_config;
use self::mcp_runtime::setup_desktop_tooling;
use self::personalization_runtime::build_personalization_prompt;
use self::provider_runtime::bootstrap_providers;
use self::settings_runtime::{default_general_settings, start_sleep_guard, stop_sleep_guard};
use self::terminal_runtime::{PtySessionState, TerminalSessionState};

pub struct DesktopRuntime {
    config: Mutex<Config>,
    db: Database,
    db_path: PathBuf,
    workspace_path: PathBuf,
    provider_registry: Mutex<Arc<ProviderRegistry>>,
    tool_registry: Mutex<Arc<ToolRegistry>>,
    mcp_resource_provider: Mutex<Option<Arc<dyn McpResourceProvider>>>,
    active_session_id: Mutex<Option<String>>,
    permission_mode: Mutex<String>,
    seq: AtomicU64,
    confirm_txs: ConfirmSenderMap,
    ask_user_txs: AskUserSenderMap,
    cancel_tokens: CancelTokenMap,
    pending_confirmations: PendingConfirmationMap,
    session_permission_rules: Arc<Mutex<HashMap<String, Vec<PermissionRule>>>>,
    terminal_sessions: Mutex<HashMap<String, TerminalSessionState>>,
    pty_sessions: Arc<Mutex<HashMap<String, PtySessionState>>>,
    general_settings: Mutex<GeneralSettings>,
    sleep_guard: Arc<Mutex<Option<Child>>>,
}

type TurnKey = (String, String);
type ConfirmSenderMap = Arc<Mutex<HashMap<TurnKey, UnboundedSender<ConfirmResponse>>>>;
type AskUserSenderMap = Arc<Mutex<HashMap<TurnKey, UnboundedSender<String>>>>;
type CancelTokenMap = Arc<Mutex<HashMap<TurnKey, tokio_util::sync::CancellationToken>>>;
type PendingConfirmationMap = Arc<Mutex<HashMap<TurnKey, PendingConfirmation>>>;

#[derive(Debug, Clone)]
struct PendingConfirmation {
    tool_name: String,
    command: Option<String>,
}

impl DesktopRuntime {
    pub fn new() -> Result<Self> {
        let workspace_path = resolve_desktop_workspace_path();
        let db_path = dirs::home_dir()
            .unwrap_or_else(|| workspace_path.clone())
            .join(".yode")
            .join("sessions.db");

        let config = load_desktop_config(&workspace_path)
            .unwrap_or_else(|_| Config::load_from(None).expect("failed to load default config"));

        let provider_registry = Mutex::new(bootstrap_providers(&config));
        let (tool_registry, mcp_resource_provider) =
            setup_desktop_tooling(&config, &workspace_path);
        if let Ok(settings) = read_desktop_settings() {
            if let Ok(browser_settings) = browser_settings_from_desktop_settings(&settings) {
                apply_browser_settings_env(&browser_settings);
            }
            if let Ok(git_settings) = git_settings_from_desktop_settings(&settings) {
                apply_git_settings_env(&git_settings);
            }
        }

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
            tool_registry: Mutex::new(tool_registry),
            mcp_resource_provider: Mutex::new(mcp_resource_provider),
            active_session_id: Mutex::new(None),
            permission_mode: Mutex::new(default_mode),
            seq: AtomicU64::new(1),
            confirm_txs: Arc::new(Mutex::new(HashMap::new())),
            ask_user_txs: Arc::new(Mutex::new(HashMap::new())),
            cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
            pending_confirmations: Arc::new(Mutex::new(HashMap::new())),
            session_permission_rules: Arc::new(Mutex::new(HashMap::new())),
            terminal_sessions: Mutex::new(HashMap::new()),
            pty_sessions: Arc::new(Mutex::new(HashMap::new())),
            general_settings: Mutex::new(default_general_settings()),
            sleep_guard: Arc::new(Mutex::new(None)),
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

    pub fn edit_diff_artifact_read(&self, path: String) -> Result<String> {
        read_edit_diff_artifact_from_roots(&path, &self.edit_diff_artifact_roots()?)
    }

    fn edit_diff_artifact_roots(&self) -> Result<Vec<PathBuf>> {
        let active_session_id = self
            .active_session_id
            .lock()
            .map_err(|_| anyhow::anyhow!("active session lock poisoned"))?
            .clone();
        let mut roots = Vec::new();
        if let Some(session_id) = active_session_id {
            if let Some(session) = self.db.get_session(&session_id)? {
                if let Some(project_root) = session.project_root {
                    if !project_root.trim().is_empty() {
                        roots.push(PathBuf::from(project_root));
                    }
                }
            }
        }
        roots.push(self.workspace_path.clone());
        roots.dedup();
        Ok(roots)
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

    pub fn import_ai_sessions(&self) -> Result<ImportAiSessionsResult> {
        let Some(paths) = rfd::FileDialog::new()
            .set_title("选择要导入的 AI 会话文件或目录")
            .add_filter("会话文件", &["json", "jsonl", "md", "markdown", "txt"])
            .pick_files()
        else {
            return Ok(ImportAiSessionsResult {
                imported: 0,
                skipped: 0,
                sessions: Vec::new(),
            });
        };

        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        let (provider, model) = self.default_llm_for_new_session(&config)?;
        drop(config);

        let mut imported_sessions = Vec::new();
        let mut skipped = 0usize;
        for file in collect_import_files(paths) {
            match import_one_ai_session(&self.db, &file, &provider, &model) {
                Ok(Some(session)) => imported_sessions.push(self.map_session(session, None)),
                Ok(None) => skipped += 1,
                Err(err) => {
                    tracing::warn!("Failed to import {}: {}", file.display(), err);
                    skipped += 1;
                }
            }
        }

        Ok(ImportAiSessionsResult {
            imported: imported_sessions.len(),
            skipped,
            sessions: imported_sessions,
        })
    }

    pub fn license_notices(&self) -> Result<Vec<LicenseNotice>> {
        Ok(read_license_notices(&self.workspace_path))
    }

    pub fn git_current_branch(&self, workspace_path: Option<String>) -> Result<Option<String>> {
        let workspace_path = workspace_path
            .map(PathBuf::from)
            .unwrap_or_else(|| self.workspace_path.clone());
        current_git_branch(&workspace_path)
    }

    pub fn worktrees_list(&self) -> Result<Vec<DesktopWorktree>> {
        list_git_worktrees(&self.workspace_path)
    }

    pub fn worktrees_prune_idle(&self) -> Result<DesktopActionResult> {
        prune_idle_worktrees(&self.workspace_path)
    }

    pub fn worktree_delete(&self, path: String) -> Result<DesktopActionResult> {
        delete_worktree(&self.workspace_path, path)
    }

    fn user_config_path(&self) -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| self.workspace_path.clone())
            .join(".yode")
            .join("config.toml")
    }

    fn project_config_path(&self) -> PathBuf {
        self.workspace_path.join(".yode").join("config.toml")
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
                self.normalize_session_llm(&mut s, &config);
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
            let mut session = session;
            self.normalize_session_llm(&mut session, &config);
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
        let personalization = self.personalization_state()?;
        context.project_memory_enabled = personalization.enable_memories
            && session
                .project_root
                .as_deref()
                .is_some_and(|root| !root.trim().is_empty());
        context.skip_tool_assisted_memory = personalization.skip_tool_chats;
        context.personalization_prompt = build_personalization_prompt(&personalization);
        context.output_style = config.ui.output_style.clone();

        let stored_msgs = self.db.load_messages(&session.id)?;
        let restored_messages: Vec<yode_llm::types::Message> = stored_msgs
            .into_iter()
            .filter_map(stored_message_to_message)
            .collect();

        let tools = self
            .tool_registry
            .lock()
            .map_err(|_| anyhow::anyhow!("tool registry lock poisoned"))?
            .clone();
        let mcp_resource_provider = self
            .mcp_resource_provider
            .lock()
            .map_err(|_| anyhow::anyhow!("mcp resource provider lock poisoned"))?
            .clone();
        let config = config.clone();
        let db_path_clone = self.db_path.clone();
        let hook_manager = build_desktop_hook_manager(&self.workspace_path)?;

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
        let should_prevent_sleep = self
            .general_settings
            .lock()
            .map(|settings| settings.prevent_sleep)
            .unwrap_or(false);
        if should_prevent_sleep {
            start_sleep_guard(&self.sleep_guard);
        }

        let confirm_txs_clone = self.confirm_txs.clone();
        let ask_user_txs_clone = self.ask_user_txs.clone();
        let cancel_tokens_clone = self.cancel_tokens.clone();
        let pending_confirmations_clone = self.pending_confirmations.clone();
        let sleep_guard_clone = self.sleep_guard.clone();

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
                if let Some(hook_manager) = hook_manager {
                    engine.set_hook_manager(hook_manager);
                }
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
                    let mapped = yode_runtime::engine_event_to_runtime_parts(event);
                    if let Some(pending_confirmation) = mapped.pending_confirmation.as_ref() {
                        if let Ok(mut pending) = pending_confirmations_clone.lock() {
                            pending.insert(
                                (session_id_str.clone(), turn_id_str.clone()),
                                PendingConfirmation {
                                    tool_name: pending_confirmation.tool_name.clone(),
                                    command: pending_confirmation.command.clone(),
                                },
                            );
                        }
                    }
                    let kind = mapped.kind;
                    let payload = mapped.payload;

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
                    if tokens.is_empty() {
                        drop(tokens);
                        stop_sleep_guard(&sleep_guard_clone);
                    }
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

fn read_edit_diff_artifact_from_roots(path: &str, roots: &[PathBuf]) -> Result<String> {
    let clean = path.trim();
    if clean.is_empty() {
        anyhow::bail!("diff artifact path is empty");
    }
    if clean.contains('\0') {
        anyhow::bail!("diff artifact path contains invalid characters");
    }

    let relative = Path::new(clean);
    if relative.is_absolute() {
        anyhow::bail!("diff artifact path must be relative");
    }
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        anyhow::bail!("diff artifact path contains unsafe components");
    }

    let mut searched = Vec::new();
    let mut last_error: Option<anyhow::Error> = None;
    let mut candidate_roots = Vec::new();
    for root in roots {
        candidate_roots.push(root.clone());
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    candidate_roots.push(path);
                }
            }
        }
    }
    candidate_roots.dedup();

    for root in &candidate_roots {
        let allowed_dir = root.join(".yode").join("edit-diffs");
        searched.push(allowed_dir.display().to_string());
        let target = root.join(relative);
        let canonical_target = match target.canonicalize() {
            Ok(path) => path,
            Err(err) => {
                last_error = Some(
                    anyhow::anyhow!(err).context(format!("Failed to access {}", target.display())),
                );
                continue;
            }
        };
        let canonical_allowed = match allowed_dir.canonicalize() {
            Ok(path) => path,
            Err(err) => {
                last_error = Some(
                    anyhow::anyhow!(err)
                        .context(format!("Failed to access {}", allowed_dir.display())),
                );
                continue;
            }
        };
        if !canonical_target.starts_with(&canonical_allowed) {
            last_error = Some(anyhow::anyhow!(
                "diff artifact path is outside .yode/edit-diffs"
            ));
            continue;
        }

        let metadata = std::fs::metadata(&canonical_target)
            .with_context(|| format!("Failed to inspect {}", canonical_target.display()))?;
        if metadata.len() > 2 * 1024 * 1024 {
            anyhow::bail!("diff artifact is too large to display");
        }

        return std::fs::read_to_string(&canonical_target)
            .with_context(|| format!("Failed to read {}", canonical_target.display()));
    }

    let searched = if searched.is_empty() {
        "no project roots".to_string()
    } else {
        searched.join(", ")
    };
    if let Some(error) = last_error {
        anyhow::bail!(
            "Failed to read diff artifact {}; searched {}; last error: {}",
            clean,
            searched,
            error
        );
    }
    anyhow::bail!(
        "Failed to read diff artifact {}; searched {}",
        clean,
        searched
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::CreateSessionRequest;
    use crate::runtime::terminal_runtime::{
        apply_terminal_color_env, parse_terminal_run_stdout, terminal_shell_command,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_config() -> Config {
        toml::from_str(include_str!("../../../../config/default.toml")).unwrap()
    }

    fn test_runtime(name: &str) -> (DesktopRuntime, PathBuf) {
        let dir = unique_temp_dir(name);
        std::fs::create_dir_all(&dir).unwrap();
        let config = test_config();
        let db_path = dir.join("sessions.db");
        let runtime = DesktopRuntime {
            config: Mutex::new(config),
            db: Database::open(&db_path).unwrap(),
            db_path,
            workspace_path: dir.clone(),
            provider_registry: Mutex::new(Arc::new(ProviderRegistry::new())),
            tool_registry: Mutex::new(Arc::new(ToolRegistry::new())),
            mcp_resource_provider: Mutex::new(None),
            active_session_id: Mutex::new(None),
            permission_mode: Mutex::new("default".to_string()),
            seq: AtomicU64::new(1),
            confirm_txs: Arc::new(Mutex::new(HashMap::new())),
            ask_user_txs: Arc::new(Mutex::new(HashMap::new())),
            cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
            pending_confirmations: Arc::new(Mutex::new(HashMap::new())),
            session_permission_rules: Arc::new(Mutex::new(HashMap::new())),
            terminal_sessions: Mutex::new(HashMap::new()),
            pty_sessions: Arc::new(Mutex::new(HashMap::new())),
            general_settings: Mutex::new(default_general_settings()),
            sleep_guard: Arc::new(Mutex::new(None)),
        };
        (runtime, dir)
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("yode-{name}-{nonce}"))
    }

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
    fn edit_diff_artifact_read_searches_session_project_roots() {
        let workspace_root = unique_temp_dir("workspace-root");
        let project_root = unique_temp_dir("project-root");
        let artifact_dir = project_root.join(".yode").join("edit-diffs");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        std::fs::write(artifact_dir.join("example.diff"), "+hello\n").unwrap();

        let content = read_edit_diff_artifact_from_roots(
            ".yode/edit-diffs/example.diff",
            &[workspace_root.clone(), project_root.clone()],
        )
        .unwrap();

        assert_eq!(content, "+hello\n");
        let _ = std::fs::remove_dir_all(workspace_root);
        let _ = std::fs::remove_dir_all(project_root);
    }

    #[test]
    fn sessions_clear_messages_removes_current_history() {
        let (runtime, dir) = test_runtime("desktop-clear-session");
        let session = runtime
            .sessions_create(CreateSessionRequest {
                title: Some("clear me".to_string()),
                project_root: None,
                provider: None,
                model: None,
            })
            .unwrap();
        runtime
            .db
            .save_message(&session.id, "user", Some("hello"), None, None, None)
            .unwrap();
        assert_eq!(
            runtime.sessions_messages(session.id.clone()).unwrap().len(),
            1
        );

        runtime.sessions_clear_messages(session.id.clone()).unwrap();

        assert!(runtime.sessions_messages(session.id).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sessions_rename_updates_session_title() {
        let (runtime, dir) = test_runtime("desktop-rename-session");
        let session = runtime
            .sessions_create(CreateSessionRequest {
                title: Some("old".to_string()),
                project_root: None,
                provider: None,
                model: None,
            })
            .unwrap();

        let renamed = runtime
            .sessions_rename(session.id.clone(), "new title".to_string())
            .unwrap();

        assert_eq!(renamed.title, "new title");
        assert_eq!(
            runtime.db.get_session(&session.id).unwrap().unwrap().name,
            Some("new title".to_string())
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sessions_export_markdown_writes_transcript() {
        let (runtime, dir) = test_runtime("desktop-export-session");
        let session = runtime
            .sessions_create(CreateSessionRequest {
                title: Some("export me".to_string()),
                project_root: Some(dir.display().to_string()),
                provider: None,
                model: None,
            })
            .unwrap();
        runtime
            .db
            .save_message(&session.id, "user", Some("hello export"), None, None, None)
            .unwrap();
        runtime
            .db
            .save_message(&session.id, "assistant", Some("hi back"), None, None, None)
            .unwrap();

        let exported = runtime.sessions_export_markdown(session.id).unwrap();
        let content = std::fs::read_to_string(&exported.path).unwrap();

        assert_eq!(exported.message_count, 2);
        assert!(content.contains("# export me"));
        assert!(content.contains("hello export"));
        assert!(content.contains("hi back"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sessions_compact_local_keeps_recent_history() {
        let (runtime, dir) = test_runtime("desktop-compact-session");
        let session = runtime
            .sessions_create(CreateSessionRequest {
                title: Some("compact me".to_string()),
                project_root: None,
                provider: None,
                model: None,
            })
            .unwrap();
        for index in 0..24 {
            let role = if index % 2 == 0 { "user" } else { "assistant" };
            runtime
                .db
                .save_message(
                    &session.id,
                    role,
                    Some(&format!("message {index}")),
                    None,
                    None,
                    None,
                )
                .unwrap();
        }

        let compacted = runtime.sessions_compact_local(session.id.clone()).unwrap();
        let messages = runtime.sessions_messages(session.id).unwrap();

        assert_eq!(compacted.before_count, 24);
        assert_eq!(compacted.after_count, 17);
        assert_eq!(messages.len(), 17);
        assert_eq!(messages[0].role, "system");
        assert!(messages[0]
            .content
            .as_deref()
            .unwrap_or_default()
            .contains("[Context summary]"));
        assert_eq!(
            messages
                .last()
                .and_then(|message| message.content.as_deref()),
            Some("message 23")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn edit_diff_artifact_read_rejects_parent_components() {
        let project_root = unique_temp_dir("project-root");
        let artifact_dir = project_root.join(".yode").join("edit-diffs");
        std::fs::create_dir_all(&artifact_dir).unwrap();

        let error = read_edit_diff_artifact_from_roots(
            ".yode/edit-diffs/../secret.diff",
            &[project_root.clone()],
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("unsafe components"));
        let _ = std::fs::remove_dir_all(project_root);
    }

    #[test]
    fn terminal_shell_uses_login_interactive_zsh() {
        let env = HashMap::from([("SHELL".to_string(), "/bin/zsh".to_string())]);
        let (shell, args) = terminal_shell_command(&env);

        assert_eq!(shell, PathBuf::from("/bin/zsh"));
        assert_eq!(args, vec!["-lic"]);
    }

    #[test]
    fn terminal_color_env_uses_truecolor_capabilities() {
        let mut command = portable_pty::CommandBuilder::new("/bin/sh");
        apply_terminal_color_env(&mut command);

        assert_eq!(
            command.get_env("TERM").and_then(|value| value.to_str()),
            Some("xterm-256color")
        );
        assert_eq!(
            command
                .get_env("COLORTERM")
                .and_then(|value| value.to_str()),
            Some("truecolor")
        );
        assert_eq!(
            command.get_env("CLICOLOR").and_then(|value| value.to_str()),
            Some("1")
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
