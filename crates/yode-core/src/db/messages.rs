use rusqlite::params;
use serde_json::Value;
use yode_llm::types::{ImageData, Message, Role};

use super::*;

impl Database {
    pub fn save_message(
        &self,
        session_id: &str,
        role: &str,
        content: Option<&str>,
        reasoning: Option<&str>,
        tool_calls_json: Option<&str>,
        tool_call_id: Option<&str>,
    ) -> Result<i64> {
        self.save_message_with_images(
            session_id,
            role,
            content,
            reasoning,
            tool_calls_json,
            tool_call_id,
            None,
        )
    }

    pub fn save_message_with_metadata(
        &self,
        session_id: &str,
        role: &str,
        content: Option<&str>,
        reasoning: Option<&str>,
        tool_calls_json: Option<&str>,
        tool_call_id: Option<&str>,
        metadata: Option<&Value>,
    ) -> Result<i64> {
        self.save_message_full(
            session_id,
            role,
            content,
            reasoning,
            tool_calls_json,
            tool_call_id,
            None,
            metadata,
        )
    }

    pub fn save_message_with_images(
        &self,
        session_id: &str,
        role: &str,
        content: Option<&str>,
        reasoning: Option<&str>,
        tool_calls_json: Option<&str>,
        tool_call_id: Option<&str>,
        images: Option<&[ImageData]>,
    ) -> Result<i64> {
        self.save_message_full(
            session_id,
            role,
            content,
            reasoning,
            tool_calls_json,
            tool_call_id,
            images,
            None,
        )
    }

    fn save_message_full(
        &self,
        session_id: &str,
        role: &str,
        content: Option<&str>,
        reasoning: Option<&str>,
        tool_calls_json: Option<&str>,
        tool_call_id: Option<&str>,
        images: Option<&[ImageData]>,
        metadata: Option<&Value>,
    ) -> Result<i64> {
        let conn = self.lock_connection()?;
        let now = Utc::now().to_rfc3339();
        let images_json = match images {
            Some(images) if !images.is_empty() => Some(serde_json::to_string(images)?),
            _ => None,
        };
        let metadata_json = metadata.map(serde_json::to_string).transpose()?;
        conn.execute(
            "INSERT INTO messages (session_id, role, content, reasoning, tool_calls_json, tool_call_id, images_json, metadata_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![session_id, role, content, reasoning, tool_calls_json, tool_call_id, images_json.as_deref(), metadata_json.as_deref(), now],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn load_messages(&self, session_id: &str) -> Result<Vec<StoredMessage>> {
        let conn = self.lock_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, reasoning, tool_calls_json, tool_call_id, images_json, metadata_json, created_at FROM messages WHERE session_id = ?1 ORDER BY id ASC",
        )?;

        let messages = stmt
            .query_map(params![session_id], |row| {
                Ok(StoredMessage {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    role: row.get(2)?,
                    content: row.get(3)?,
                    reasoning: row.get(4)?,
                    tool_calls_json: row.get(5)?,
                    tool_call_id: row.get(6)?,
                    images_json: row.get(7)?,
                    metadata_json: row.get(8)?,
                    created_at: parse_rfc3339_or_now(row.get::<_, String>(9).unwrap_or_default()),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(messages)
    }

    pub fn replace_messages(&self, session_id: &str, messages: &[Message]) -> Result<()> {
        let mut conn = self.lock_connection()?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM messages WHERE session_id = ?1",
            params![session_id],
        )?;

        let now = Utc::now().to_rfc3339();
        {
            let mut stmt = tx.prepare(
                "INSERT INTO messages (session_id, role, content, reasoning, tool_calls_json, tool_call_id, images_json, metadata_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;

            for message in messages {
                let tool_calls_json = if message.tool_calls.is_empty() {
                    None
                } else {
                    Some(serde_json::to_string(&message.tool_calls)?)
                };
                let images_json = if message.images.is_empty() {
                    None
                } else {
                    Some(serde_json::to_string(&message.images)?)
                };

                stmt.execute(params![
                    session_id,
                    role_label(&message.role),
                    message.content.as_deref(),
                    message.reasoning.as_deref(),
                    tool_calls_json.as_deref(),
                    message.tool_call_id.as_deref(),
                    images_json.as_deref(),
                    None::<&str>,
                    now,
                ])?;
            }
        }

        tx.commit()?;
        Ok(())
    }
}

fn role_label(role: &Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
        Role::System => "system",
    }
}
