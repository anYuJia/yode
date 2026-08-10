mod messages;
mod records;
mod sessions;
#[cfg(test)]
mod tests;
mod turns;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::Connection;

use crate::session::Session;
use crate::session_lock::SessionLock;

pub use records::{SessionArtifacts, SessionListEntry, StoredMessage};
pub use turns::{
    redact_event_payload, TurnEvent, TurnRecord, TurnState, MAX_TURN_EVENTS, MAX_TURN_EVENT_BYTES,
};

/// SQLite-backed session and message store.
/// Uses an internal Mutex to make it Send+Sync safe.
pub struct Database {
    pub(super) conn: Mutex<Connection>,
    /// 数据库文件的规范化绝对路径：跨进程会话锁、诊断等按此路径推导。
    db_path: PathBuf,
}

/// 单个 SQLite 只读事务内读取的会话与全部消息快照（导出等一致性读取专用）。
#[derive(Debug, Clone)]
pub struct SessionSnapshot {
    pub session: Session,
    pub messages: Vec<StoredMessage>,
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
            db_path: crate::session_lock::normalize_db_path(path),
        };
        db.init_tables()?;
        Ok(db)
    }

    /// 数据库文件的规范化绝对路径。
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// 获取该数据库路径下指定 session 的跨进程生命周期锁（RAII）。
    /// 同一 session 已被其他进程持有时返回简体中文错误，绝不阻塞。
    pub fn session_lock(&self, session_id: &str) -> Result<SessionLock> {
        SessionLock::acquire(&self.db_path, session_id)
    }

    /// 在单个 SQLite 只读事务内同时读取 session 元数据与全部 messages，
    /// 保证导出/恢复读取到的是操作前或操作后的完整一致快照。
    /// session 不存在时返回 `None`。
    pub fn load_session_snapshot(&self, session_id: &str) -> Result<Option<SessionSnapshot>> {
        let conn = self.lock_connection()?;
        conn.execute_batch("BEGIN;")
            .with_context(|| "无法开启导出快照事务")?;
        let snapshot = self.read_session_snapshot(&conn, session_id);
        match snapshot {
            Ok(value) => {
                conn.execute_batch("COMMIT;")
                    .with_context(|| "导出快照事务提交失败")?;
                Ok(value)
            }
            Err(err) => {
                let _ = conn.execute_batch("ROLLBACK;");
                Err(err)
            }
        }
    }

    fn read_session_snapshot(
        &self,
        conn: &Connection,
        session_id: &str,
    ) -> Result<Option<SessionSnapshot>> {
        let mut stmt = conn.prepare(
            "SELECT id, name, project_root, provider, model, created_at, updated_at FROM sessions WHERE id = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![session_id])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let session = Session {
            id: row.get(0)?,
            name: row.get(1)?,
            project_root: row.get(2)?,
            provider: row.get(3)?,
            model: row.get(4)?,
            created_at: parse_rfc3339_strict(row.get::<_, String>(5)?)?,
            updated_at: parse_rfc3339_strict(row.get::<_, String>(6)?)?,
        };

        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, reasoning, tool_calls_json, tool_call_id, images_json, metadata_json, sort_order, created_at FROM messages WHERE session_id = ?1 ORDER BY sort_order ASC, id ASC",
        )?;
        let messages = stmt
            .query_map(rusqlite::params![session_id], |row| {
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

        Ok(Some(SessionSnapshot { session, messages }))
    }

    pub(super) fn lock_connection(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| anyhow::anyhow!("database connection lock poisoned"))
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
                sort_order INTEGER NOT NULL DEFAULT 0,
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
        migrate_message_sort_order(&conn)?;
        migrate_turn_journal(&conn)?;
        Ok(())
    }
}

/// Turn journal schema 迁移（user_version = 7）：
/// 新增 turns 与 turn_events 表。主键覆盖完整 session_id + turn_id（+ seq），
/// 禁止只按短 ID 查询；旧数据不受影响，迁移失败整体回滚。
fn migrate_turn_journal(conn: &Connection) -> Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if current >= 7 {
        return Ok(());
    }
    conn.execute_batch("BEGIN IMMEDIATE;")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS turns (
            session_id TEXT NOT NULL,
            turn_id TEXT NOT NULL,
            status TEXT NOT NULL,
            started_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            ended_at TEXT,
            last_seq INTEGER NOT NULL DEFAULT -1,
            cancellation_requested INTEGER NOT NULL DEFAULT 0,
            detail TEXT,
            error_code TEXT,
            PRIMARY KEY (session_id, turn_id),
            FOREIGN KEY (session_id) REFERENCES sessions(id)
        );
        CREATE TABLE IF NOT EXISTS turn_events (
            session_id TEXT NOT NULL,
            turn_id TEXT NOT NULL,
            seq INTEGER NOT NULL,
            kind TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            PRIMARY KEY (session_id, turn_id, seq),
            FOREIGN KEY (session_id) REFERENCES sessions(id)
        );
        CREATE INDEX IF NOT EXISTS idx_turn_events_session ON turn_events(session_id);
        CREATE INDEX IF NOT EXISTS idx_turns_session ON turns(session_id);
        PRAGMA user_version = 7; COMMIT;",
    )
    .map_err(|err| {
        let _ = conn.execute_batch("ROLLBACK;");
        anyhow::anyhow!(err)
    })
    .with_context(|| "Failed to migrate turn journal schema to version 7")?;
    Ok(())
}

fn migrate_message_sort_order(conn: &Connection) -> Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if current >= 6 {
        return Ok(());
    }
    conn.execute_batch("BEGIN IMMEDIATE;")?;
    let result = ensure_column(
        conn,
        "messages",
        "sort_order",
        "sort_order INTEGER NOT NULL DEFAULT 0",
    )
    .and_then(|_| {
        conn.execute(
            "UPDATE messages SET sort_order = id WHERE sort_order = 0",
            [],
        )?;
        conn.execute_batch("PRAGMA user_version = 6; COMMIT;")?;
        Ok(())
    });
    if result.is_err() {
        let _ = conn.execute_batch("ROLLBACK;");
    }
    result
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
