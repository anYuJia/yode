use std::path::PathBuf;

use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio::sync::mpsc::unbounded_channel;
use uuid::Uuid;

use yode_core::config::Config;
use yode_core::context::AgentContext;
use yode_core::db::Database;
use yode_core::engine::AgentEngine;
use yode_core::session::Session;

use super::{
    personalization_runtime::build_personalization_prompt,
    provider_runtime::normalized_provider_model, turn_runtime::configure_desktop_permissions,
    DesktopRuntime,
};
use crate::hook_settings::build_desktop_hook_manager;
use crate::protocol::{
    CreateSessionRequest, DesktopImageOutput, DesktopMessage, DesktopSession, SessionCompactResult,
    SessionExportResult,
};
use crate::session_helpers::{
    build_local_compaction_summary, render_session_markdown, short_session_id, stored_images,
    stored_message_to_message,
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

    pub fn sessions_clear_messages(&self, session_id: String) -> Result<()> {
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
        let session = self
            .db
            .get_session(&session_id)?
            .ok_or_else(|| anyhow::anyhow!("session '{}' not found", session_id))?;
        let messages = self.db.load_messages(&session_id)?;
        let root = session
            .project_root
            .as_deref()
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
            .unwrap_or_else(|| self.workspace_path.clone());
        let export_dir = root.join(".yode").join("exports");
        tokio::fs::create_dir_all(&export_dir).await?;
        let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
        let path = export_dir.join(format!(
            "{}-{}.md",
            short_session_id(&session_id),
            timestamp
        ));
        tokio::fs::write(&path, render_session_markdown(&session, &messages)).await?;
        Ok(SessionExportResult {
            path: path.display().to_string(),
            message_count: messages.len(),
        })
    }

    pub fn sessions_compact_local(&self, session_id: String) -> Result<SessionCompactResult> {
        const KEEP_LAST_MESSAGES: usize = 16;

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

        let workspace_path = session
            .project_root
            .as_deref()
            .filter(|root| !root.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.workspace_path.clone());
        let mut permissions = configure_desktop_permissions(&config, &workspace_path);
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
            workspace_path,
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

        let restored_messages = self
            .db
            .load_messages(&session.id)?
            .into_iter()
            .filter_map(stored_message_to_message)
            .collect();
        let hook_manager = build_desktop_hook_manager(&self.workspace_path)?;
        let db = Database::open(&self.db_path)?;
        let mut engine = AgentEngine::new(provider, tools, permissions, context);
        engine.set_database(db);
        if let Some(hook_manager) = hook_manager {
            engine.set_hook_manager(hook_manager);
        }
        if let Some(mcp) = mcp_resource_provider {
            engine.set_mcp_resource_provider(mcp);
        }
        engine.set_mcp_resource_policy(yode_tools::tool::McpResourcePolicy {
            allow: config.mcp.resource_allow.clone(),
            deny: config.mcp.resource_deny.clone(),
        });
        engine.restore_messages(restored_messages);

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
        self.db.delete_session(&session_id)?;
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
