use std::path::PathBuf;
use std::sync::atomic::Ordering;

use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::json;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc::unbounded_channel;
use uuid::Uuid;

use yode_core::config::Config;
use yode_core::context::AgentContext;
use yode_core::db::Database;
use yode_core::engine::{AgentEngine, ConfirmResponse, EngineEvent};
use yode_core::permission::{PermissionManager, PermissionRule, RuleBehavior, RuleSource};
use yode_core::session::Session;

use super::{
    personalization_runtime::build_personalization_prompt,
    settings_runtime::{start_sleep_guard, stop_sleep_guard},
    DesktopRuntime, PendingConfirmation,
};
use crate::hook_settings::build_desktop_hook_manager;
use crate::protocol::{DesktopEvent, SendMessageRequest, TurnAccepted};
use crate::session_helpers::{stored_message_to_message, title_from_content_or_images};

impl DesktopRuntime {
    pub async fn turn_send_message(
        &self,
        app: AppHandle,
        request: SendMessageRequest,
    ) -> Result<TurnAccepted> {
        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?
            .clone();
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
        let personalization = self.personalization_state().await?;
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
        let db_path_clone = self.db_path.clone();
        let hook_manager = build_desktop_hook_manager(&self.workspace_path).await?;

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
                        emit_desktop_event(&app, desktop_event);
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
                engine.restore_messages_async(restored_messages).await;

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
                        if let Err(send_err) = error_event_tx.send(EngineEvent::Error(err.to_string())) {
                            tracing::warn!(
                                error = %send_err,
                                "Failed to enqueue engine error event from desktop turn task"
                            );
                        }
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
                            emit_desktop_event(&app, desktop_event);
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

                    emit_desktop_event(&app, desktop_event);
                    seq += 1;
                }

                if let Err(err) = handle.await {
                    tracing::error!("Desktop turn task join failed: {}", err);
                }

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
            .get(&(session_id.clone(), turn_id.clone()))
            .cloned();
        if let Some(tx) = tx {
            let response = if allow && always_allow {
                ConfirmResponse::AllowAlways
            } else if allow {
                ConfirmResponse::Allow
            } else {
                ConfirmResponse::Deny
            };
            if let Err(err) = tx.send(response) {
                tracing::warn!(
                    session_id = %session_id,
                    turn_id = %turn_id,
                    error = ?err,
                    "Failed to send desktop permission response"
                );
            }
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
        if let Some(tx) = txs.get(&(session_id.clone(), turn_id.clone())) {
            if let Err(err) = tx.send(answer) {
                tracing::warn!(
                    session_id = %session_id,
                    turn_id = %turn_id,
                    error = %err,
                    "Failed to send ask-user response"
                );
            }
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

fn emit_desktop_event(app: &AppHandle, desktop_event: DesktopEvent) {
    let session_id = desktop_event.session_id.clone();
    let turn_id = desktop_event.turn_id.clone();
    let kind = desktop_event.kind.clone();
    if let Err(err) = app.emit("desktop-event", desktop_event) {
        tracing::warn!(
            session_id = %session_id,
            turn_id = %turn_id,
            kind = %kind,
            error = %err,
            "Failed to emit desktop runtime event"
        );
    }
}

pub(super) fn configure_desktop_permissions(
    config: &Config,
    _workdir: &std::path::Path,
) -> PermissionManager {
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
