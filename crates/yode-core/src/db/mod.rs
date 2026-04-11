mod messages;
mod records;
mod sessions;
#[cfg(test)]
mod tests;

use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::Connection;

pub use records::{SessionArtifacts, SessionListEntry, StoredMessage};

/// SQLite-backed session and message store.
/// Uses an internal Mutex to make it Send+Sync safe.
pub struct Database {
    pub(super) conn: Mutex<Connection>,
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

        let _ = conn.execute("ALTER TABLE messages ADD COLUMN reasoning TEXT", []);
        Ok(())
    }
}

pub(super) fn parse_rfc3339_or_now(value: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&value)
        .map(|datetime| datetime.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}
