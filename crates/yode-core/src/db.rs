use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use yode_llm::types::{Message, Role};

use crate::session::Session;

/// SQLite-backed session and message store.
/// Uses an internal Mutex to make it Send+Sync safe.
pub struct Database {
    conn: Mutex<Connection>,
}

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
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionArtifacts {
    pub last_compaction_mode: Option<String>,
    pub last_compaction_at: Option<String>,
    pub last_compaction_summary_excerpt: Option<String>,
    pub last_compaction_session_memory_path: Option<String>,
    pub last_compaction_transcript_path: Option<String>,
    pub last_session_memory_update_at: Option<String>,
    pub last_session_memory_update_path: Option<String>,
    pub last_session_memory_generated_summary: bool,
}

#[derive(Debug, Clone)]
pub struct SessionListEntry {
    pub session: Session,
    pub artifacts: SessionArtifacts,
}

impl Database {
    /// Open or create the database at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open database at '{}'", path.display()))?;

        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_tables()?;
        Ok(db)
    }

    fn init_tables(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                name TEXT,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT,
                reasoning TEXT,
                tool_calls_json TEXT,
                tool_call_id TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (session_id) REFERENCES sessions(id)
            );
            CREATE TABLE IF NOT EXISTS session_artifacts (
                session_id TEXT PRIMARY KEY,
                last_compaction_mode TEXT,
                last_compaction_at TEXT,
                last_compaction_summary_excerpt TEXT,
                last_compaction_session_memory_path TEXT,
                last_compaction_transcript_path TEXT,
                last_session_memory_update_at TEXT,
                last_session_memory_update_path TEXT,
                last_session_memory_generated_summary INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (session_id) REFERENCES sessions(id)
            );
            CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);",
        )?;

        // Migration: add reasoning column if it doesn't exist.
        // We reuse the 'conn' guard acquired above to avoid deadlocks.
        let _ = conn.execute("ALTER TABLE messages ADD COLUMN reasoning TEXT", []);

        Ok(())
    }

    pub fn create_session(&self, session: &Session) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sessions (id, name, provider, model, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                session.id,
                session.name,
                session.provider,
                session.model,
                session.created_at.to_rfc3339(),
                session.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn touch_session(&self, session_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
            params![now, session_id],
        )?;
        Ok(())
    }

    pub fn get_session(&self, session_id: &str) -> Result<Option<Session>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, provider, model, created_at, updated_at FROM sessions WHERE id = ?1",
        )?;

        let mut rows = stmt.query(params![session_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Session {
                id: row.get(0)?,
                name: row.get(1)?,
                provider: row.get(2)?,
                model: row.get(3)?,
                created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_sessions(&self, limit: usize) -> Result<Vec<Session>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, provider, model, created_at, updated_at FROM sessions ORDER BY updated_at DESC LIMIT ?1",
        )?;

        let sessions = stmt
            .query_map(params![limit as i64], |row| {
                Ok(Session {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    provider: row.get(2)?,
                    model: row.get(3)?,
                    created_at: DateTime::parse_from_rfc3339(
                        &row.get::<_, String>(4).unwrap_or_default(),
                    )
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                    updated_at: DateTime::parse_from_rfc3339(
                        &row.get::<_, String>(5).unwrap_or_default(),
                    )
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(sessions)
    }

    pub fn list_sessions_with_artifacts(&self, limit: usize) -> Result<Vec<SessionListEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT
                s.id, s.name, s.provider, s.model, s.created_at, s.updated_at,
                a.last_compaction_mode, a.last_compaction_at, a.last_compaction_summary_excerpt,
                a.last_compaction_session_memory_path, a.last_compaction_transcript_path,
                a.last_session_memory_update_at, a.last_session_memory_update_path,
                a.last_session_memory_generated_summary
             FROM sessions s
             LEFT JOIN session_artifacts a ON a.session_id = s.id
             ORDER BY s.updated_at DESC
             LIMIT ?1",
        )?;

        let entries = stmt
            .query_map(params![limit as i64], |row| {
                Ok(SessionListEntry {
                    session: Session {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        provider: row.get(2)?,
                        model: row.get(3)?,
                        created_at: DateTime::parse_from_rfc3339(
                            &row.get::<_, String>(4).unwrap_or_default(),
                        )
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                        updated_at: DateTime::parse_from_rfc3339(
                            &row.get::<_, String>(5).unwrap_or_default(),
                        )
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    },
                    artifacts: SessionArtifacts {
                        last_compaction_mode: row.get(6)?,
                        last_compaction_at: row.get(7)?,
                        last_compaction_summary_excerpt: row.get(8)?,
                        last_compaction_session_memory_path: row.get(9)?,
                        last_compaction_transcript_path: row.get(10)?,
                        last_session_memory_update_at: row.get(11)?,
                        last_session_memory_update_path: row.get(12)?,
                        last_session_memory_generated_summary: row
                            .get::<_, Option<i64>>(13)?
                            .unwrap_or(0)
                            != 0,
                    },
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(entries)
    }

    pub fn upsert_session_artifacts(
        &self,
        session_id: &str,
        artifacts: &SessionArtifacts,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO session_artifacts (
                session_id,
                last_compaction_mode,
                last_compaction_at,
                last_compaction_summary_excerpt,
                last_compaction_session_memory_path,
                last_compaction_transcript_path,
                last_session_memory_update_at,
                last_session_memory_update_path,
                last_session_memory_generated_summary,
                updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(session_id) DO UPDATE SET
                last_compaction_mode = excluded.last_compaction_mode,
                last_compaction_at = excluded.last_compaction_at,
                last_compaction_summary_excerpt = excluded.last_compaction_summary_excerpt,
                last_compaction_session_memory_path = excluded.last_compaction_session_memory_path,
                last_compaction_transcript_path = excluded.last_compaction_transcript_path,
                last_session_memory_update_at = excluded.last_session_memory_update_at,
                last_session_memory_update_path = excluded.last_session_memory_update_path,
                last_session_memory_generated_summary = excluded.last_session_memory_generated_summary,
                updated_at = excluded.updated_at",
            params![
                session_id,
                artifacts.last_compaction_mode,
                artifacts.last_compaction_at,
                artifacts.last_compaction_summary_excerpt,
                artifacts.last_compaction_session_memory_path,
                artifacts.last_compaction_transcript_path,
                artifacts.last_session_memory_update_at,
                artifacts.last_session_memory_update_path,
                if artifacts.last_session_memory_generated_summary {
                    1
                } else {
                    0
                },
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn save_message(
        &self,
        session_id: &str,
        role: &str,
        content: Option<&str>,
        reasoning: Option<&str>,
        tool_calls_json: Option<&str>,
        tool_call_id: Option<&str>,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO messages (session_id, role, content, reasoning, tool_calls_json, tool_call_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![session_id, role, content, reasoning, tool_calls_json, tool_call_id, now],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn load_messages(&self, session_id: &str) -> Result<Vec<StoredMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, reasoning, tool_calls_json, tool_call_id, created_at FROM messages WHERE session_id = ?1 ORDER BY id ASC",
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
                    created_at: DateTime::parse_from_rfc3339(
                        &row.get::<_, String>(7).unwrap_or_default(),
                    )
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(messages)
    }

    pub fn replace_messages(&self, session_id: &str, messages: &[Message]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM messages WHERE session_id = ?1",
            params![session_id],
        )?;

        let now = Utc::now().to_rfc3339();
        {
            let mut stmt = tx.prepare(
                "INSERT INTO messages (session_id, role, content, reasoning, tool_calls_json, tool_call_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;

            for message in messages {
                let role = match message.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                    Role::System => "system",
                };
                let tool_calls_json = if message.tool_calls.is_empty() {
                    None
                } else {
                    Some(serde_json::to_string(&message.tool_calls)?)
                };

                stmt.execute(params![
                    session_id,
                    role,
                    message.content.as_deref(),
                    message.reasoning.as_deref(),
                    tool_calls_json.as_deref(),
                    message.tool_call_id.as_deref(),
                    now,
                ])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    /// Update session name
    pub fn update_session_name(&self, session_id: &str, name: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sessions SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, Utc::now().to_rfc3339(), session_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use tempfile::tempdir;
    use yode_llm::types::Message;

    use super::{Database, SessionArtifacts};
    use crate::session::Session;

    #[test]
    fn replace_messages_overwrites_previous_session_history() {
        let temp = tempdir().unwrap();
        let db = Database::open(&temp.path().join("sessions.db")).unwrap();
        db.create_session(&Session {
            id: "session-1".to_string(),
            name: None,
            provider: "mock".to_string(),
            model: "mock-model".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .unwrap();

        db.save_message("session-1", "user", Some("old"), None, None, None)
            .unwrap();
        db.save_message("session-1", "assistant", Some("older"), None, None, None)
            .unwrap();

        db.replace_messages(
            "session-1",
            &[
                Message::user("new user"),
                Message::assistant("new assistant"),
            ],
        )
        .unwrap();

        let messages = db.load_messages("session-1").unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content.as_deref(), Some("new user"));
        assert_eq!(messages[1].content.as_deref(), Some("new assistant"));
    }

    #[test]
    fn upsert_session_artifacts_persists_and_lists_metadata() {
        let temp = tempdir().unwrap();
        let db = Database::open(&temp.path().join("sessions.db")).unwrap();
        db.create_session(&Session {
            id: "session-1".to_string(),
            name: Some("demo".to_string()),
            provider: "mock".to_string(),
            model: "mock-model".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .unwrap();

        db.upsert_session_artifacts(
            "session-1",
            &SessionArtifacts {
                last_compaction_mode: Some("manual".to_string()),
                last_compaction_at: Some("2026-01-01 10:00:00".to_string()),
                last_compaction_summary_excerpt: Some("summary".to_string()),
                last_compaction_session_memory_path: Some("/tmp/session.md".to_string()),
                last_compaction_transcript_path: Some("/tmp/transcript.md".to_string()),
                last_session_memory_update_at: Some("2026-01-01 10:05:00".to_string()),
                last_session_memory_update_path: Some("/tmp/live.md".to_string()),
                last_session_memory_generated_summary: true,
            },
        )
        .unwrap();

        let sessions = db.list_sessions_with_artifacts(10).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(
            sessions[0].artifacts.last_compaction_mode.as_deref(),
            Some("manual")
        );
        assert_eq!(
            sessions[0]
                .artifacts
                .last_compaction_transcript_path
                .as_deref(),
            Some("/tmp/transcript.md")
        );
        assert!(sessions[0].artifacts.last_session_memory_generated_summary);
    }
}
