use rusqlite::params;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
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

    #[allow(clippy::too_many_arguments)]
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

    #[allow(clippy::too_many_arguments)]
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

    #[allow(clippy::too_many_arguments)]
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
            "INSERT INTO messages (session_id, role, content, reasoning, tool_calls_json, tool_call_id, images_json, metadata_json, sort_order, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, COALESCE((SELECT MAX(sort_order) + 1 FROM messages WHERE session_id = ?1), 0), ?9)",
            params![session_id, role, content, reasoning, tool_calls_json, tool_call_id, images_json.as_deref(), metadata_json.as_deref(), now],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn load_messages(&self, session_id: &str) -> Result<Vec<StoredMessage>> {
        let conn = self.lock_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, reasoning, tool_calls_json, tool_call_id, images_json, metadata_json, sort_order, created_at FROM messages WHERE session_id = ?1 ORDER BY sort_order ASC, id ASC",
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
                    sort_order: row.get(9)?,
                    created_at: parse_rfc3339_strict(row.get::<_, String>(10).unwrap_or_default())
                        .map_err(|err| rusqlite_corruption(10, err))?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(messages)
    }

    /// Atomically rewrite a session snapshot while retaining the identity and metadata of
    /// messages that remain in the conversation. The returned ids correspond to `messages`.
    pub fn replace_messages(&self, session_id: &str, messages: &[Message]) -> Result<Vec<i64>> {
        let mut conn = self.lock_connection()?;
        let tx = conn.transaction()?;
        let mut existing = HashMap::new();
        {
            let mut stmt = tx.prepare(
                "SELECT id, session_id, tool_calls_json, images_json FROM messages WHERE session_id = ?1",
            )?;
            let rows = stmt.query_map(params![session_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })?;
            for row in rows {
                let (id, owner, tool_calls_json, images_json) = row?;
                existing.insert(id, (owner, tool_calls_json, images_json));
            }
        }

        let mut retained_ids = HashSet::new();
        let mut result_ids = Vec::with_capacity(messages.len());
        let now = Utc::now().to_rfc3339();
        for (position, message) in messages.iter().enumerate() {
            let tool_calls_json = encode_tool_calls(message)?;
            let images_json = encode_images(message)?;
            if let Some(id) = message.storage_id {
                if !retained_ids.insert(id) {
                    anyhow::bail!("session snapshot contains duplicate message id {id}");
                }
                let Some((owner, old_tool_calls, old_images)) = existing.get(&id) else {
                    anyhow::bail!("message id {id} does not belong to session '{session_id}'");
                };
                if owner != session_id {
                    anyhow::bail!("message id {id} belongs to another session");
                }
                let tool_calls_json =
                    preserve_json_encoding(old_tool_calls.as_deref(), tool_calls_json);
                let images_json = preserve_json_encoding(old_images.as_deref(), images_json);
                tx.execute(
                    "UPDATE messages SET role = ?1, content = ?2, reasoning = ?3, tool_calls_json = ?4, tool_call_id = ?5, images_json = ?6, sort_order = ?7 WHERE id = ?8 AND session_id = ?9",
                    params![role_label(&message.role), message.content, message.reasoning, tool_calls_json, message.tool_call_id, images_json, position as i64, id, session_id],
                )?;
                result_ids.push(id);
            } else {
                tx.execute(
                    "INSERT INTO messages (session_id, role, content, reasoning, tool_calls_json, tool_call_id, images_json, metadata_json, sort_order, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9)",
                    params![session_id, role_label(&message.role), message.content, message.reasoning, tool_calls_json, message.tool_call_id, images_json, position as i64, now],
                )?;
                result_ids.push(tx.last_insert_rowid());
            }
        }

        for id in existing.keys().filter(|id| !retained_ids.contains(id)) {
            tx.execute(
                "DELETE FROM messages WHERE id = ?1 AND session_id = ?2",
                params![id, session_id],
            )?;
        }

        tx.commit()?;
        Ok(result_ids)
    }
}

fn encode_tool_calls(message: &Message) -> Result<Option<String>> {
    (!message.tool_calls.is_empty())
        .then(|| serde_json::to_string(&message.tool_calls).map_err(Into::into))
        .transpose()
}

fn encode_images(message: &Message) -> Result<Option<String>> {
    (!message.images.is_empty())
        .then(|| serde_json::to_string(&message.images).map_err(Into::into))
        .transpose()
}

/// Keep the exact original JSON when a decoded snapshot did not change its value. If the old
/// value is malformed, retain it rather than replacing it with an empty fallback.
fn preserve_json_encoding(old: Option<&str>, new: Option<String>) -> Option<String> {
    let Some(old) = old else { return new };
    let Some(new_value) = new.as_deref() else {
        return serde_json::from_str::<Value>(old)
            .is_err()
            .then(|| old.to_string());
    };
    match (
        serde_json::from_str::<Value>(old),
        serde_json::from_str::<Value>(new_value),
    ) {
        (Ok(old), Ok(new)) if old == new => Some(old.to_string()),
        (Err(_), _) => Some(old.to_string()),
        _ => new,
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
