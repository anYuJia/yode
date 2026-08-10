use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use yode_llm::types::{ContentBlock, Message, Role};

use crate::session::Session;

/// A stored message in a session.
#[derive(Debug, Clone)]
pub struct StoredMessage {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub content: Option<String>,
    pub reasoning: Option<String>,
    pub tool_calls_json: Option<String>,
    pub tool_call_id: Option<String>,
    pub images_json: Option<String>,
    pub metadata_json: Option<String>,
    pub sort_order: i64,
    pub created_at: DateTime<Utc>,
}

impl StoredMessage {
    /// Decode the common database representation used by both CLI and desktop restore paths.
    /// Invalid optional JSON is reported and reduced to an empty field, while the stable storage
    /// id remains attached so a later snapshot rewrite cannot delete or overwrite the raw value.
    pub fn to_message(&self) -> Option<Message> {
        let role = match self.role.as_str() {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            "tool" => Role::Tool,
            "system" => Role::System,
            other => {
                tracing::warn!(
                    message_id = self.id,
                    role = other,
                    "数据库消息角色未知，跳过恢复"
                );
                return None;
            }
        };
        let tool_calls = self
            .tool_calls_json
            .as_deref()
            .and_then(|json| self.parse_json("tool_calls_json", json))
            .unwrap_or_default();
        let images = self
            .images_json
            .as_deref()
            .and_then(|json| self.parse_json("images_json", json))
            .unwrap_or_default();
        if let Some(json) = self.metadata_json.as_deref() {
            let _ = self.parse_json::<serde_json::Value>("metadata_json", json);
        }
        let mut content_blocks = Vec::new();
        if let Some(reasoning) = self.reasoning.as_deref() {
            content_blocks.push(ContentBlock::Thinking {
                thinking: reasoning.to_string(),
                signature: None,
            });
        }
        if let Some(content) = self.content.as_deref() {
            content_blocks.push(ContentBlock::Text {
                text: content.to_string(),
            });
        }
        Some(
            Message {
                storage_id: Some(self.id),
                role,
                content: self.content.clone(),
                content_blocks,
                reasoning: self.reasoning.clone(),
                tool_calls,
                tool_call_id: self.tool_call_id.clone(),
                images,
            }
            .normalized(),
        )
    }

    fn parse_json<T: DeserializeOwned>(&self, field: &str, json: &str) -> Option<T> {
        match serde_json::from_str(json) {
            Ok(value) => Some(value),
            Err(error) => {
                tracing::warn!(
                    message_id = self.id,
                    session_id = %self.session_id,
                    field,
                    error = %error,
                    "数据库消息 JSON 损坏，保留原始值"
                );
                None
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SessionArtifacts {
    pub last_compaction_mode: Option<String>,
    pub last_compaction_at: Option<String>,
    pub last_compaction_summary_excerpt: Option<String>,
    pub last_compaction_session_memory_path: Option<String>,
    pub last_compaction_transcript_path: Option<String>,
    pub last_compact_boundary_json: Option<String>,
    pub last_session_memory_update_at: Option<String>,
    pub last_session_memory_update_path: Option<String>,
    pub last_session_memory_generated_summary: bool,
}

#[derive(Debug, Clone)]
pub struct SessionListEntry {
    pub session: Session,
    pub artifacts: SessionArtifacts,
}
