mod messages;
mod records;
mod sessions;
#[cfg(test)]
mod tests;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

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
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create database parent dir '{}'",
                    parent.display()
                )
            })?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open database at '{}'", path.display()))?;
        // 生产连接强制开启外键、WAL 与忙等待，确保并发与完整性语义稳定。
        conn.pragma_update(None, "foreign_keys", "ON")
            .with_context(|| "Failed to enable foreign keys")?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .with_context(|| "Failed to enable WAL journal mode")?;
        conn.pragma_update(None, "busy_timeout", 5_000)
            .with_context(|| "Failed to set busy timeout")?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .with_context(|| "Failed to set synchronous mode")?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_tables()?;
        Ok(db)
    }

    fn init_tables(&self) -> Result<()> {
        let conn = self.lock_connection()?;
        // 迁移必须在事务中执行：user_version 记录当前 schema 版本，
        // 任何一步失败都会整体回滚，不会留下半迁移状态。
        conn.execute_batch(
            "BEGIN IMMEDIATE;
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                name TEXT,
                project_root TEXT,
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
                images_json TEXT,
                metadata_json TEXT,
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
                last_compact_boundary_json TEXT,
                last_session_memory_update_at TEXT,
                last_session_memory_update_path TEXT,
                last_session_memory_generated_summary INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (session_id) REFERENCES sessions(id)
            );
            CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
            COMMIT;",
        )?;

        // schema 迁移步骤：每步在自身事务内执行并递增 user_version。
        migrate_ensure_column(&conn, 1, "messages", "reasoning", "reasoning TEXT")?;
        migrate_ensure_column(&conn, 2, "messages", "images_json", "images_json TEXT")?;
        migrate_ensure_column(&conn, 3, "messages", "metadata_json", "metadata_json TEXT")?;
        migrate_ensure_column(&conn, 4, "sessions", "project_root", "project_root TEXT")?;
        migrate_ensure_column(
            &conn,
            5,
            "session_artifacts",
            "last_compact_boundary_json",
            "last_compact_boundary_json TEXT",
        )?;
        Ok(())
    }

    pub(super) fn lock_connection(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| anyhow::anyhow!("database connection lock poisoned"))
    }
}

fn ensure_column(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
    column_definition: &str,
) -> Result<()> {
    if column_exists(conn, table_name, column_name)? {
        return Ok(());
    }
    conn.execute(
        &format!("ALTER TABLE {table_name} ADD COLUMN {column_definition}"),
        [],
    )
    .with_context(|| format!("Failed to migrate database column {table_name}.{column_name}"))?;
    Ok(())
}

/// 在事务内执行一次 schema 迁移：只有迁移成功才把 user_version 推进到
/// `version`，失败整体回滚。旧库（user_version=0 且缺列）也会被逐步升级。
fn migrate_ensure_column(
    conn: &Connection,
    version: i64,
    table_name: &str,
    column_name: &str,
    column_definition: &str,
) -> Result<()> {
    let current: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .with_context(|| "Failed to read schema version")?;
    if current >= version {
        return Ok(());
    }
    conn.execute_batch("BEGIN IMMEDIATE;")?;
    let result = ensure_column(conn, table_name, column_name, column_definition).and_then(|_| {
        conn.execute_batch(&format!("PRAGMA user_version = {version}; COMMIT;"))
            .with_context(|| format!("Failed to bump schema version to {version}"))
    });
    if result.is_err() {
        let _ = conn.execute_batch("ROLLBACK;");
    }
    result
}

fn column_exists(conn: &Connection, table_name: &str, column_name: &str) -> Result<bool> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table_name})"))
        .with_context(|| format!("Failed to inspect database table '{table_name}'"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column_name {
            return Ok(true);
        }
    }
    Ok(false)
}

/// 严格解析 RFC3339 时间戳：损坏的时间戳必须作为数据损坏向上报告，
/// 绝不静默替换为当前时间（DB-003）。
pub(super) fn parse_rfc3339_strict(value: String) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value)
        .map(|datetime| datetime.with_timezone(&Utc))
        .with_context(|| format!("数据库中的时间戳损坏，无法解析: '{value}'"))
}

/// 把数据损坏错误转换为 rusqlite 行解析错误，使读取路径整体失败并暴露根因。
fn rusqlite_corruption(index: usize, error: anyhow::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, error.into())
}
