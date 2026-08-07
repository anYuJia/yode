use std::collections::HashSet;

use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::json;
use tauri::AppHandle;
use tokio::sync::mpsc::unbounded_channel;
use uuid::Uuid;

use yode_core::db::Database;
use yode_core::engine::{AgentEngine, ConfirmResponse, EngineEvent};
use yode_core::permission::{PermissionRule, RuleBehavior, RuleSource};
use yode_core::session::Session;

use super::{
    engine_setup::{
        build_desktop_agent_context, build_session_permissions, configure_engine_services,
        restore_messages_from_stored, session_workspace_path,
    },
    settings_system::start_sleep_guard,
    turn_events::emit_desktop_event,
    turn_loop::run_desktop_turn_event_loop,
    DesktopRuntime,
};
use crate::hook_settings::build_desktop_hook_manager;
use crate::protocol::{DesktopEvent, SendMessageRequest, SessionRunState, TurnAccepted};
use crate::session_helpers::title_from_content_or_images;

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
        let (session, turn_slot) = if let Some(session_id) = request
            .session_id
            .as_deref()
            .filter(|id| !id.trim().is_empty())
        {
            // 同一会话禁止并发 turn：原子地检查并占用 in-flight 槽位
            // （同一把锁内完成检查+插入，两个并发请求不可能同时通过）。
            // 槽位一直保持到 turn 事件循环 quiesce 之后才释放，
            // 取消请求期间新 turn 一律被拒绝，杜绝旧工具与新 turn 并发。
            let slot = SessionTurnSlot::acquire(&self.active_sessions, session_id)?;
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
            (s, Some(slot))
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
            // 新会话首轮同样占用 in-flight 占位：首轮取消后、旧引擎 quiesce 期间，
            // 新 turn 必须被拒绝（uuid 天然唯一，acquire 必成功）
            let slot = SessionTurnSlot::acquire(&self.active_sessions, &session.id)?;
            (session, Some(slot))
        };

        self.set_active_session(session.id.clone())?;
        self.db.touch_session(&session.id)?;
        let accepted_session = self.map_session(session.clone(), Some(session.id.as_str()));

        let turn_id = Uuid::new_v4().to_string();
        let session_id = session.id.clone();
        let emit_turn_id = turn_id.clone();
        // seq 从 0 开始，在每个 turn 内严格单调递增（不使用跨 turn 的固定编号区间）
        let seq_base = 0;

        let provider = self
            .provider_registry
            .lock()
            .map_err(|_| anyhow::anyhow!("registry lock poisoned"))?
            .get(&session.provider)
            .ok_or_else(|| {
                anyhow::anyhow!("Provider '{}' not found in registry", session.provider)
            })?;

        let turn_workspace_path = session_workspace_path(&session, &self.workspace_path);

        let permissions = build_session_permissions(
            &config,
            &turn_workspace_path,
            &self.permission_mode,
            &self.session_permission_rules,
            &session.id,
        );
        let personalization = self.personalization_state().await?;
        let context =
            build_desktop_agent_context(&session, turn_workspace_path, &config, &personalization);

        let stored_msgs = self.db.load_messages(&session.id)?;
        let restored_messages = restore_messages_from_stored(stored_msgs);

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
        let hook_manager =
            build_desktop_hook_manager(&self.workspace_path, self.workspace_trusted()).await?;

        let (confirm_tx, confirm_rx) = unbounded_channel::<ConfirmResponse>();
        {
            let mut txs = self
                .confirm_txs
                .lock()
                .map_err(|_| anyhow::anyhow!("poisoned"))?;
            txs.insert((session_id.clone(), emit_turn_id.clone()), confirm_tx);
        }

        let (ask_user_query_tx, ask_user_query_rx) =
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
        // 事件循环需要在取消时观察 token，需要额外一份 clone
        let loop_cancel_token = cancel_token.clone();
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
        // 占位槽位在 turn 事件循环 quiesce 后释放；此处先解除 Drop 清理，
        // 避免 spawn 后局部 guard 提前释放导致并发保护失效
        if let Some(mut slot) = turn_slot {
            slot.disarm();
        }
        let active_sessions_clone = self.active_sessions.clone();
        let run_registry_clone = self.run_registry.clone();
        update_run_state(
            &self.run_registry,
            &session_id,
            &emit_turn_id,
            "running",
            None,
        );

        std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(err) => {
                    tracing::error!("Failed to create tokio runtime: {}", err);
                    // 线程启动失败：释放占位与 token，避免会话被永久判定为运行中
                    release_turn_occupancy(
                        &active_sessions_clone,
                        &cancel_tokens_clone,
                        &session_id,
                        &emit_turn_id,
                    );
                    update_run_state(
                        &run_registry_clone,
                        &session_id,
                        &emit_turn_id,
                        "failed",
                        Some(format!("无法创建后台运行时：{err}")),
                    );
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
                        emit_desktop_event(&app, desktop_event);
                        // 后台启动失败：释放占位与 token，避免会话被永久判定为运行中
                        release_turn_occupancy(
                            &active_sessions_clone,
                            &cancel_tokens_clone,
                            &session_id,
                            &emit_turn_id,
                        );
                        update_run_state(
                            &run_registry_clone,
                            &session_id,
                            &emit_turn_id,
                            "failed",
                            Some(err.to_string()),
                        );
                        return;
                    }
                };
                engine.set_database(db_clone);
                configure_engine_services(
                    &mut engine,
                    hook_manager,
                    mcp_resource_provider,
                    &config,
                );
                engine.set_ask_user_channels(ask_user_query_tx, ask_user_answer_rx);
                engine.restore_messages_async(restored_messages).await;

                let (event_tx, event_rx) = unbounded_channel::<EngineEvent>();
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
                        if let Err(send_err) =
                            error_event_tx.send(EngineEvent::Error(err.to_string()))
                        {
                            tracing::warn!(
                                error = %send_err,
                                "Failed to enqueue engine error event from desktop turn task"
                            );
                        }
                    }
                });

                run_desktop_turn_event_loop(
                    app.clone(),
                    session_id.clone(),
                    emit_turn_id.clone(),
                    seq_base,
                    event_rx,
                    ask_user_query_rx,
                    handle,
                    confirm_txs_clone,
                    ask_user_txs_clone,
                    cancel_tokens_clone,
                    pending_confirmations_clone,
                    sleep_guard_clone,
                    loop_cancel_token,
                    active_sessions_clone,
                    run_registry_clone,
                )
                .await;
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
        let tx = self
            .confirm_txs
            .lock()
            .map_err(|_| anyhow::anyhow!("poisoned"))?
            .get(&(session_id.clone(), turn_id.clone()))
            .cloned();
        let Some(tx) = tx else {
            anyhow::bail!("该权限请求已失效或对应任务已结束。");
        };
        let response = if allow && always_allow {
            ConfirmResponse::AllowAlways
        } else if allow {
            ConfirmResponse::Allow
        } else {
            ConfirmResponse::Deny
        };
        tx.send(response)
            .map_err(|_| anyhow::anyhow!("权限回复发送失败，任务可能已经结束。"))?;
        let pending_request = self
            .pending_confirmations
            .lock()
            .map_err(|_| anyhow::anyhow!("poisoned"))?
            .remove(&(session_id.clone(), turn_id.clone()));

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
        update_run_state(&self.run_registry, &session_id, &turn_id, "running", None);
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
        let tx = txs
            .get(&(session_id.clone(), turn_id.clone()))
            .ok_or_else(|| anyhow::anyhow!("该问题已失效或对应任务已结束。"))?;
        tx.send(answer)
            .map_err(|_| anyhow::anyhow!("问题回复发送失败，请重试。"))?;
        drop(txs);
        update_run_state(&self.run_registry, &session_id, &turn_id, "running", None);
        Ok(())
    }

    pub fn turn_cancel(&self, session_id: String, turn_id: String) -> Result<()> {
        let tokens = self
            .cancel_tokens
            .lock()
            .map_err(|_| anyhow::anyhow!("poisoned"))?;
        // 只触发取消，不删除 token：token 与 in-flight 占位由 turn 事件循环
        // 在 quiesce 后统一释放，取消请求期间新 turn 仍会被拒绝
        if let Some(token) = tokens.get(&(session_id, turn_id)) {
            token.clone().cancel();
        }
        Ok(())
    }

    /// 取消入口：先发出 cancelling 事件，由 turn 事件循环在引擎停止后
    /// 发出 cancelled 终态（Cancelling → 后端确认停止 → Cancelled）。
    pub fn turn_cancel_request(
        &self,
        app: AppHandle,
        session_id: String,
        turn_id: String,
    ) -> Result<()> {
        let _ = app;
        update_run_state(
            &self.run_registry,
            &session_id,
            &turn_id,
            "cancelling",
            None,
        );
        self.turn_cancel(session_id, turn_id)
    }
}

pub(super) fn update_run_state(
    registry: &std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, SessionRunState>>>,
    session_id: &str,
    turn_id: &str,
    status: &str,
    detail: Option<String>,
) {
    if let Ok(mut runs) = registry.lock() {
        runs.insert(
            session_id.to_string(),
            SessionRunState {
                session_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
                status: status.to_string(),
                updated_at: Utc::now().to_rfc3339(),
                detail,
            },
        );
    }
}

/// 每会话 in-flight 占位：原子检查+占用（同一把锁内完成检查与插入，
/// 两个并发请求不可能同时通过）。
/// Drop 时若未 disarm 会自动释放（失败路径兜底）；
/// 成功路径调用 disarm() 后由 turn 事件循环在 quiesce 后释放。
#[derive(Debug)]
pub(super) struct SessionTurnSlot {
    active: std::sync::Arc<std::sync::Mutex<HashSet<String>>>,
    session_id: String,
    armed: bool,
}

impl SessionTurnSlot {
    pub(super) fn acquire(
        active: &std::sync::Arc<std::sync::Mutex<HashSet<String>>>,
        session_id: &str,
    ) -> anyhow::Result<Self> {
        let mut set = active
            .lock()
            .map_err(|_| anyhow::anyhow!("active session lock poisoned"))?;
        if !set.insert(session_id.to_string()) {
            anyhow::bail!("该会话已有进行中的任务，请等待完成或取消后再发送。");
        }
        Ok(Self {
            active: active.clone(),
            session_id: session_id.to_string(),
            armed: true,
        })
    }

    pub(super) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for SessionTurnSlot {
    fn drop(&mut self) {
        if self.armed {
            if let Ok(mut set) = self.active.lock() {
                set.remove(&self.session_id);
            }
        }
    }
}

/// 释放 turn 的 in-flight 占位与 cancel token（幂等）。
/// 由 turn 事件循环收尾、以及后台线程启动失败路径共用，
/// 保证任何路径都不会让会话被永久判定为运行中。
pub(super) fn release_turn_occupancy(
    active_sessions: &std::sync::Arc<std::sync::Mutex<HashSet<String>>>,
    cancel_tokens: &super::turn_loop::CancelTokenMap,
    session_id: &str,
    turn_id: &str,
) {
    if let Ok(mut active) = active_sessions.lock() {
        active.remove(session_id);
    }
    if let Ok(mut tokens) = cancel_tokens.lock() {
        tokens.remove(&(session_id.to_string(), turn_id.to_string()));
    }
}
