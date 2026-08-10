use std::path::PathBuf;

use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio::sync::mpsc::unbounded_channel;
use uuid::Uuid;

use yode_core::config::Config;
use yode_core::db::Database;
use yode_core::engine::AgentEngine;
use yode_core::session::Session;

use super::{
    engine_setup::{
        build_desktop_agent_context, build_session_permissions, configure_engine_services,
        restore_messages_from_stored, session_workspace_path,
    },
    provider_runtime::normalized_provider_model,
    turn_runtime::{SessionOperation, SessionOperationSlot},
    DesktopRuntime,
};
use crate::hook_settings::build_desktop_hook_manager;
use crate::protocol::{
    CreateSessionRequest, DesktopImageOutput, DesktopMessage, DesktopSession, SessionCompactResult,
    SessionExportResult,
};
use crate::session_helpers::{
    build_local_compaction_summary, export_session_token, render_session_markdown, stored_images,
    stored_message_to_message, stored_metadata,
};

impl DesktopRuntime {
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
        let mut session = Session {
            id: Uuid::new_v4().to_string(),
            name: request.title.or_else(|| Some("桌面端会话".to_string())),
            project_root: request.project_root,
            provider: request.provider.unwrap_or(default_provider),
            model: request.model.unwrap_or(default_model),
            created_at: now,
            updated_at: now,
        };
        self.normalize_session_llm(&mut session, &config);

        self.db.create_session(&session)?;
        self.set_active_session(session.id.clone())?;
        Ok(self.map_session(session, None))
    }

    pub fn sessions_messages(&self, session_id: String) -> Result<Vec<DesktopMessage>> {
        Ok(self
            .db
            .load_messages(&session_id)?
            .into_iter()
            .map(|message| {
                let images = stored_images(&message)
                    .into_iter()
                    .map(|image| DesktopImageOutput {
                        base64: image.base64,
                        media_type: image.media_type,
                    })
                    .collect();
                let metadata = stored_metadata(&message);
                DesktopMessage {
                    sort_order: Some(message.sort_order),
                    images,
                    id: message.id,
                    role: message.role,
                    content: message.content,
                    reasoning: message.reasoning,
                    tool_calls_json: message.tool_calls_json,
                    tool_call_id: message.tool_call_id,
                    metadata,
                    created_at: message.created_at.to_rfc3339(),
                }
            })
            .collect())
    }

    pub fn sessions_clear_messages(&self, session_id: String) -> Result<()> {
        let _slot = SessionOperationSlot::acquire(
            &self.active_sessions,
            &session_id,
            SessionOperation::ClearMessages,
        )?;
        // 跨进程锁：与 CLI/其他桌面进程的 turn、压缩、删除互斥
        let _session_lock = self.db.session_lock(&session_id)?;
        if self.db.get_session(&session_id)?.is_none() {
            anyhow::bail!("session '{}' not found", session_id);
        }
        self.db.replace_messages(&session_id, &[])?;
        self.db.touch_session(&session_id)?;
        Ok(())
    }

    pub fn sessions_rename(&self, session_id: String, title: String) -> Result<DesktopSession> {
        let title = title.trim();
        if title.is_empty() {
            anyhow::bail!("session title cannot be empty");
        }
        self.db.update_session_name(&session_id, title)?;
        let session = self
            .db
            .get_session(&session_id)?
            .ok_or_else(|| anyhow::anyhow!("session '{}' not found", session_id))?;
        let active_session_id = self
            .active_session_id
            .lock()
            .map_err(|_| anyhow::anyhow!("active session lock poisoned"))?
            .clone();
        Ok(self.map_session(session, active_session_id.as_deref()))
    }

    pub async fn sessions_export_markdown(
        &self,
        session_id: String,
    ) -> Result<SessionExportResult> {
        let _slot = SessionOperationSlot::acquire(
            &self.active_sessions,
            &session_id,
            SessionOperation::Export,
        )?;
        // 跨进程锁：导出与 turn/clear/compact/delete 互斥，
        // 保证导出的是操作前或操作后的完整一致快照
        let _session_lock = self.db.session_lock(&session_id)?;
        let snapshot = self
            .db
            .load_session_snapshot(&session_id)?
            .ok_or_else(|| anyhow::anyhow!("session '{}' not found", session_id))?;
        let root = snapshot
            .session
            .project_root
            .as_deref()
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
            .unwrap_or_else(|| self.workspace_path.clone());
        let export_dir = root.join(".yode").join("exports");
        let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
        let base = format!("{}-{}", export_session_token(&session_id), timestamp);
        // 唯一命名 + 临时文件原子替换：同一秒连续导出不互相覆盖，
        // 写入失败不产生半截文件、不触碰已有导出
        let path = yode_core::session_lock::write_unique_export_file(
            &export_dir,
            &base,
            &render_session_markdown(&snapshot.session, &snapshot.messages),
        )?;
        Ok(SessionExportResult {
            path: path.display().to_string(),
            message_count: snapshot.messages.len(),
        })
    }

    pub fn sessions_compact_local(&self, session_id: String) -> Result<SessionCompactResult> {
        const KEEP_LAST_MESSAGES: usize = 16;

        let _slot = SessionOperationSlot::acquire(
            &self.active_sessions,
            &session_id,
            SessionOperation::CompactLocal,
        )?;
        // 跨进程锁：与 CLI/其他桌面进程的 turn、clear、delete 互斥
        let _session_lock = self.db.session_lock(&session_id)?;

        let session = self
            .db
            .get_session(&session_id)?
            .ok_or_else(|| anyhow::anyhow!("session '{}' not found", session_id))?;
        let messages = self.db.load_messages(&session_id)?;
        let before_count = messages.len();
        if before_count <= KEEP_LAST_MESSAGES + 1 {
            return Ok(SessionCompactResult {
                before_count,
                after_count: before_count,
                removed_count: 0,
                summary: "当前会话还不需要压缩。".to_string(),
            });
        }

        let split_at = before_count.saturating_sub(KEEP_LAST_MESSAGES);
        let (older, recent) = messages.split_at(split_at);
        let summary = build_local_compaction_summary(&session, older);
        let mut compacted = Vec::with_capacity(recent.len() + 1);
        compacted.push(yode_llm::types::Message::system(summary.clone()));
        compacted.extend(
            recent
                .iter()
                .filter_map(|message| stored_message_to_message(message.clone())),
        );
        self.db.replace_messages(&session_id, &compacted)?;
        self.db.touch_session(&session_id)?;

        Ok(SessionCompactResult {
            before_count,
            after_count: compacted.len(),
            removed_count: before_count.saturating_sub(compacted.len()),
            summary,
        })
    }

    pub async fn sessions_compact_engine(
        &self,
        session_id: String,
    ) -> Result<SessionCompactResult> {
        let _slot = SessionOperationSlot::acquire(
            &self.active_sessions,
            &session_id,
            SessionOperation::CompactEngine,
        )?;
        // 跨进程锁：与 CLI/其他桌面进程的 turn、clear、delete 互斥，
        // 在引擎压缩全过程中持有
        let _session_lock = self.db.session_lock(&session_id)?;
        let session = self
            .db
            .get_session(&session_id)?
            .ok_or_else(|| anyhow::anyhow!("session '{}' not found", session_id))?;
        let before_count = self.db.load_messages(&session_id)?.len();

        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?
            .clone();
        let provider = self
            .provider_registry
            .lock()
            .map_err(|_| anyhow::anyhow!("registry lock poisoned"))?
            .get(&session.provider)
            .ok_or_else(|| {
                anyhow::anyhow!("Provider '{}' not found in registry", session.provider)
            })?;
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

        let workspace_path = session_workspace_path(&session, &self.workspace_path);
        let permissions = build_session_permissions(
            &config,
            &workspace_path,
            &self.permission_mode,
            &self.session_permission_rules,
            &session.id,
        );
        let personalization = self.personalization_state().await?;
        let context =
            build_desktop_agent_context(&session, workspace_path, &config, &personalization);

        let restored_messages = restore_messages_from_stored(self.db.load_messages(&session.id)?);
        let hook_manager =
            build_desktop_hook_manager(&self.workspace_path, self.workspace_trusted()).await?;
        let db = Database::open(&self.db_path)?;
        let mut engine = AgentEngine::new(provider, tools, permissions, context);
        engine.set_database(db);
        configure_engine_services(&mut engine, hook_manager, mcp_resource_provider, &config);
        engine.restore_messages_async(restored_messages).await;

        let (event_tx, _event_rx) = unbounded_channel();
        let compacted = engine.force_compact(event_tx).await;
        let runtime = engine.runtime_state();
        let after_count = self.db.load_messages(&session_id)?.len();
        let summary = if compacted {
            runtime
                .last_compaction_summary_excerpt
                .unwrap_or_else(|| "已完成 engine-level 手动压缩。".to_string())
        } else {
            "当前会话还不需要压缩。".to_string()
        };
        Ok(SessionCompactResult {
            before_count,
            after_count,
            removed_count: before_count.saturating_sub(after_count),
            summary,
        })
    }

    pub fn sessions_delete(&self, session_id: String) -> Result<()> {
        let _slot = SessionOperationSlot::acquire(
            &self.active_sessions,
            &session_id,
            SessionOperation::Delete,
        )?;
        // 跨进程锁：与 CLI/其他桌面进程的 turn、clear、compact、export 互斥
        let _session_lock = self.db.session_lock(&session_id)?;
        let project_root = self
            .db
            .get_session(&session_id)?
            .and_then(|session| session.project_root)
            .filter(|root| !root.trim().is_empty())
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
            .unwrap_or_else(|| self.workspace_path.clone());
        self.db.delete_session(&session_id)?;
        // 会话删除成功后清理磁盘工件：仅清理已验证属于该会话的新工件，
        // 绝不误删其他会话工件或旧共享文件；失败记录可诊断错误但不回滚删除。
        match yode_core::session_memory::cleanup_session_artifacts(&project_root, &session_id) {
            Ok(report) => {
                tracing::info!(
                    "会话 {} 删除完成，已清理 {} 个磁盘工件",
                    session_id,
                    report.removed_files
                );
            }
            Err(err) => {
                tracing::error!(
                    "会话 {} 已从数据库删除，但磁盘工件清理失败: {}",
                    session_id,
                    err
                );
            }
        }
        Ok(())
    }

    pub fn sessions_update_llm(
        &self,
        session_id: String,
        provider: String,
        model: String,
    ) -> Result<()> {
        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        let (provider, model) = normalized_provider_model(&config, &provider, &model);
        self.db.update_session_llm(&session_id, &provider, &model)?;
        Ok(())
    }

    pub(super) fn set_active_session(&self, session_id: String) -> Result<()> {
        *self
            .active_session_id
            .lock()
            .map_err(|_| anyhow::anyhow!("active session lock poisoned"))? = Some(session_id);
        Ok(())
    }

    pub(super) fn map_session(
        &self,
        session: Session,
        active_session_id: Option<&str>,
    ) -> DesktopSession {
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

    pub(super) fn default_llm_for_new_session(&self, config: &Config) -> Result<(String, String)> {
        if let Some(session) = self.db.list_sessions(1)?.into_iter().next() {
            if !session.provider.trim().is_empty() && !session.model.trim().is_empty() {
                let (provider, model) =
                    normalized_provider_model(config, &session.provider, &session.model);
                return Ok((provider, model));
            }
        }
        Ok(normalized_provider_model(
            config,
            &config.llm.default_provider,
            &config.llm.default_model,
        ))
    }

    pub(super) fn normalize_session_llm(&self, session: &mut Session, config: &Config) {
        let (provider, model) =
            normalized_provider_model(config, &session.provider, &session.model);
        session.provider = provider;
        session.model = model;
    }
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

fn relative_time(updated_at: DateTime<Utc>) -> String {
    let local_time = updated_at.with_timezone(&chrono::Local);
    local_time.format("%m月%d日 %H:%M").to_string()
}
