use std::path::PathBuf;

use anyhow::Result;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use yode_core::config::Config;
use yode_core::session::Session;

use super::{normalized_provider_model, DesktopRuntime};
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

    pub fn sessions_export_markdown(&self, session_id: String) -> Result<SessionExportResult> {
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
        std::fs::create_dir_all(&export_dir)?;
        let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
        let path = export_dir.join(format!(
            "{}-{}.md",
            short_session_id(&session_id),
            timestamp
        ));
        std::fs::write(&path, render_session_markdown(&session, &messages))?;
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
