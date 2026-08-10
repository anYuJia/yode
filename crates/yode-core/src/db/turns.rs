use chrono::{DateTime, Utc};
use rusqlite::params;
use serde_json::Value;

use super::*;

/// Turn 状态必须是明确的有限集合；非法状态字符串不得静默接受。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnState {
    Starting,
    Running,
    WaitingApproval,
    WaitingUser,
    Cancelling,
    Completed,
    Cancelled,
    Failed,
    Interrupted,
}

impl TurnState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Running => "running",
            Self::WaitingApproval => "waiting_approval",
            Self::WaitingUser => "waiting_user",
            Self::Cancelling => "cancelling",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }

    /// 解析状态字符串；非法值返回 None，绝不静默接受。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "starting" => Some(Self::Starting),
            "running" => Some(Self::Running),
            "waiting_approval" => Some(Self::WaitingApproval),
            "waiting_user" => Some(Self::WaitingUser),
            "cancelling" => Some(Self::Cancelling),
            "completed" => Some(Self::Completed),
            "cancelled" => Some(Self::Cancelled),
            "failed" => Some(Self::Failed),
            "interrupted" => Some(Self::Interrupted),
            _ => None,
        }
    }

    /// 终态：终态不可被普通事件重新打开。
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Cancelled | Self::Failed | Self::Interrupted
        )
    }
}

/// turns 表的一行（持久化 turn journal 的事实来源）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRecord {
    pub session_id: String,
    pub turn_id: String,
    pub status: TurnState,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub last_seq: i64,
    pub cancellation_requested: bool,
    pub detail: Option<String>,
    pub error_code: Option<String>,
}

/// turn_events 表的一行。payload_json 在落盘前必须经过脱敏。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnEvent {
    pub session_id: String,
    pub turn_id: String,
    pub seq: i64,
    pub kind: String,
    pub timestamp: DateTime<Utc>,
    pub payload_json: String,
}

/// 每个 turn 的事件日志上限：事件条数与 payload 总字节数。
pub const MAX_TURN_EVENTS: i64 = 500;
pub const MAX_TURN_EVENT_BYTES: usize = 256 * 1024;

impl Database {
    /// 创建 turn 记录（初始状态 starting、seq = -1）。
    /// 幂等：同一 (session_id, turn_id) 已存在时返回既有记录。
    pub fn create_turn(&self, session_id: &str, turn_id: &str) -> Result<TurnRecord> {
        let conn = self.lock_connection()?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO turns (session_id, turn_id, status, started_at, updated_at, last_seq, cancellation_requested)
             VALUES (?1, ?2, 'starting', ?3, ?3, -1, 0)
             ON CONFLICT(session_id, turn_id) DO NOTHING",
            params![session_id, turn_id, now],
        )?;
        let mut stmt = conn.prepare(
            "SELECT session_id, turn_id, status, started_at, updated_at, ended_at, last_seq, cancellation_requested, detail, error_code
             FROM turns WHERE session_id = ?1 AND turn_id = ?2",
        )?;
        let record = stmt
            .query_row(params![session_id, turn_id], read_turn_row)
            .map_err(|err| match err {
                rusqlite::Error::QueryReturnedNoRows => {
                    anyhow::anyhow!("turn '{turn_id}' for session '{session_id}' was not created")
                }
                other => other.into(),
            })?;
        Ok(record)
    }

    /// 在同一个数据库临界区（单事务）内追加事件并推进 last_seq：
    /// - seq 必须严格大于当前 last_seq，否则拒绝（重复/乱序写入）；
    /// - 事件必须属于已存在的 turn，否则拒绝（跨会话/幽灵写入）；
    /// - payload 在落盘前统一脱敏。
    ///
    /// 状态不随事件变化时使用 None，状态更新走 `append_turn_event_with_state`。
    pub fn append_turn_event(&self, event: &TurnEvent) -> Result<()> {
        self.append_turn_event_with_state(event, None, None, None)
    }

    /// 事件内容、事件 seq 与生命周期状态在同一事务内更新：
    /// 追加事件并推进 last_seq，同时把 turn 状态更新为 `state`（终态冻结语义与
    /// `update_turn_state` 一致，且与事件写入共享同一临界区）。
    pub fn append_turn_event_with_state(
        &self,
        event: &TurnEvent,
        state: Option<TurnState>,
        detail: Option<String>,
        error_code: Option<String>,
    ) -> Result<()> {
        let mut conn = self.lock_connection()?;
        let tx = conn.transaction()?;
        let current: i64 = tx.query_row(
            "SELECT last_seq FROM turns WHERE session_id = ?1 AND turn_id = ?2",
            params![event.session_id, event.turn_id],
            |row| row.get(0),
        )?;
        if event.seq <= current {
            anyhow::bail!(
                "turn '{}' 的事件 seq {} 不满足单调递增要求（当前 last_seq = {}）",
                event.turn_id,
                event.seq,
                current
            );
        }
        let redacted = redact_event_payload_json(&event.payload_json)?;
        tx.execute(
            "INSERT INTO turn_events (session_id, turn_id, seq, kind, timestamp, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event.session_id,
                event.turn_id,
                event.seq,
                event.kind,
                event.timestamp.to_rfc3339(),
                redacted
            ],
        )?;
        tx.execute(
            "UPDATE turns SET last_seq = ?1, updated_at = ?2
             WHERE session_id = ?3 AND turn_id = ?4",
            params![
                event.seq,
                Utc::now().to_rfc3339(),
                event.session_id,
                event.turn_id
            ],
        )?;
        if let Some(state) = state {
            let current_status = read_turn_status(&tx, &event.session_id, &event.turn_id)?;
            if current_status.is_terminal() && state != current_status {
                anyhow::bail!(
                    "turn '{}' 已处于终态 {}，不可重新打开为 {}",
                    event.turn_id,
                    current_status.as_str(),
                    state.as_str()
                );
            }
            tx.execute(
                "UPDATE turns SET status = ?1, updated_at = ?2,
                 ended_at = CASE WHEN ?3 THEN ?2 ELSE ended_at END,
                 detail = COALESCE(?4, detail), error_code = COALESCE(?5, error_code)
                 WHERE session_id = ?6 AND turn_id = ?7",
                params![
                    state.as_str(),
                    Utc::now().to_rfc3339(),
                    state.is_terminal(),
                    detail,
                    error_code,
                    event.session_id,
                    event.turn_id
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// 更新 turn 状态（与事件写入共用临界区语义）。
    /// - 非法状态字符串返回 None 输入错误；
    /// - 终态不可被重新打开；
    /// - 进入终态时记录 ended_at。
    pub fn update_turn_state(
        &self,
        session_id: &str,
        turn_id: &str,
        status: TurnState,
        detail: Option<String>,
        error_code: Option<String>,
    ) -> Result<TurnRecord> {
        let mut conn = self.lock_connection()?;
        let tx = conn.transaction()?;
        let current = read_turn_status(&tx, session_id, turn_id)?;
        if current.is_terminal() && status != current {
            anyhow::bail!(
                "turn '{}' 已处于终态 {}，不可重新打开为 {}",
                turn_id,
                current.as_str(),
                status.as_str()
            );
        }
        let ended_at = if status.is_terminal() {
            Some(Utc::now().to_rfc3339())
        } else {
            None
        };
        tx.execute(
            "UPDATE turns SET status = ?1, updated_at = ?2, ended_at = COALESCE(?3, ended_at), detail = COALESCE(?4, detail), error_code = COALESCE(?5, error_code)
             WHERE session_id = ?6 AND turn_id = ?7",
            params![
                status.as_str(),
                Utc::now().to_rfc3339(),
                ended_at,
                detail,
                error_code,
                session_id,
                turn_id
            ],
        )?;
        let record = read_turn(&tx, session_id, turn_id)?;
        tx.commit()?;
        Ok(record)
    }

    /// 记录取消请求（不改变状态；状态流转由事件循环负责）。
    pub fn mark_turn_cancellation_requested(&self, session_id: &str, turn_id: &str) -> Result<()> {
        let conn = self.lock_connection()?;
        conn.execute(
            "UPDATE turns SET cancellation_requested = 1, updated_at = ?1
             WHERE session_id = ?2 AND turn_id = ?3",
            params![Utc::now().to_rfc3339(), session_id, turn_id],
        )?;
        Ok(())
    }

    /// 列出指定会话的 turn（按 started_at 降序）。
    pub fn list_turns(&self, session_id: &str) -> Result<Vec<TurnRecord>> {
        let conn = self.lock_connection()?;
        let mut stmt = conn.prepare(
            "SELECT session_id, turn_id, status, started_at, updated_at, ended_at, last_seq, cancellation_requested, detail, error_code
             FROM turns WHERE session_id = ?1 ORDER BY started_at DESC",
        )?;
        let rows = stmt
            .query_map(params![session_id], read_turn_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 跨会话列出最近 turn（bootstrap runs_list 的事实来源，按 updated_at 降序）。
    pub fn list_recent_turns(&self, limit: usize) -> Result<Vec<TurnRecord>> {
        let conn = self.lock_connection()?;
        let mut stmt = conn.prepare(
            "SELECT session_id, turn_id, status, started_at, updated_at, ended_at, last_seq, cancellation_requested, detail, error_code
             FROM turns ORDER BY updated_at DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit as i64], read_turn_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 读取 turn 记录（不存在返回 None）。
    pub fn get_turn(&self, session_id: &str, turn_id: &str) -> Result<Option<TurnRecord>> {
        let conn = self.lock_connection()?;
        let mut stmt = conn.prepare(
            "SELECT session_id, turn_id, status, started_at, updated_at, ended_at, last_seq, cancellation_requested, detail, error_code
             FROM turns WHERE session_id = ?1 AND turn_id = ?2",
        )?;
        let mut rows = stmt.query(params![session_id, turn_id])?;
        match rows.next()? {
            Some(row) => Ok(Some(read_turn_row(row)?)),
            None => Ok(None),
        }
    }

    /// 读取某个 turn 中 seq > since_seq 的事件（升序）。limit 为 0 或 None 表示不限制。
    pub fn list_turn_events_since(
        &self,
        session_id: &str,
        turn_id: &str,
        since_seq: i64,
        limit: Option<usize>,
    ) -> Result<Vec<TurnEvent>> {
        let conn = self.lock_connection()?;
        let mut stmt = conn.prepare(
            "SELECT session_id, turn_id, seq, kind, timestamp, payload_json
             FROM turn_events
             WHERE session_id = ?1 AND turn_id = ?2 AND seq > ?3
             ORDER BY seq ASC
             LIMIT ?4",
        )?;
        let rows = stmt
            .query_map(
                params![
                    session_id,
                    turn_id,
                    since_seq,
                    limit.unwrap_or(i64::MAX as usize) as i64
                ],
                read_turn_event_row,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 读取某个 turn 最近 N 条事件（降序；调用方自行反转展示顺序）。
    pub fn list_recent_turn_events(
        &self,
        session_id: &str,
        turn_id: &str,
        limit: usize,
    ) -> Result<Vec<TurnEvent>> {
        let conn = self.lock_connection()?;
        let mut stmt = conn.prepare(
            "SELECT session_id, turn_id, seq, kind, timestamp, payload_json
             FROM turn_events
             WHERE session_id = ?1 AND turn_id = ?2
             ORDER BY seq DESC
             LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![session_id, turn_id, limit as i64], |row| {
                read_turn_event_row(row)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 删除会话的全部 turn journal（turns + turn_events）。
    /// 由会话删除路径在同一事务内调用，失败返回可诊断错误。
    pub fn delete_turn_journal(&self, conn: &Connection, session_id: &str) -> Result<()> {
        conn.execute(
            "DELETE FROM turn_events WHERE session_id = ?1",
            params![session_id],
        )?;
        conn.execute(
            "DELETE FROM turns WHERE session_id = ?1",
            params![session_id],
        )?;
        Ok(())
    }

    /// 进程启动时调用：旧的 starting/running/waiting_*/cancelling 状态一律标记为
    /// interrupted，绝不伪装成成功；保留诊断信息（detail + error_code）。
    /// 返回被标记的 turn 数量。
    pub fn mark_interrupted_turns(&self, detail: &str) -> Result<usize> {
        let mut conn = self.lock_connection()?;
        let tx = conn.transaction()?;
        let stale = tx
            .prepare(
                "SELECT session_id, turn_id FROM turns
                 WHERE status IN ('starting', 'running', 'waiting_approval', 'waiting_user', 'cancelling')",
            )?
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let now = Utc::now().to_rfc3339();
        for (session_id, turn_id) in &stale {
            tx.execute(
                "UPDATE turns SET status = 'interrupted', updated_at = ?1, ended_at = ?1,
                 detail = ?2, error_code = 'interrupted_on_startup'
                 WHERE session_id = ?3 AND turn_id = ?4",
                params![now, detail, session_id, turn_id],
            )?;
        }
        tx.commit()?;
        Ok(stale.len())
    }

    /// 事件日志限量清理：仅清理已终态 turn 的超限事件。
    /// 保留 turn_started、终态事件与 error 事件（essential）；超出条数/字节上限时，
    /// 从最早的普通事件开始丢弃，最新的事件保留。绝不触碰运行中的 turn。
    /// 返回清理掉的事件条数。
    pub fn prune_turn_journals(&self) -> Result<usize> {
        let mut conn = self.lock_connection()?;
        let tx = conn.transaction()?;
        let terminal = tx
            .prepare(
                "SELECT session_id, turn_id, last_seq FROM turns
                 WHERE status IN ('completed', 'cancelled', 'failed', 'interrupted')",
            )?
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut pruned = 0usize;
        for (session_id, turn_id, last_seq) in terminal {
            // 条数和字节数都未超限时无需清理
            if last_seq < MAX_TURN_EVENTS {
                continue;
            }
            let mut stmt = tx.prepare(
                "SELECT seq, kind, length(payload_json) AS bytes FROM turn_events
                 WHERE session_id = ?1 AND turn_id = ?2 ORDER BY seq ASC",
            )?;
            let events = stmt
                .query_map(params![session_id, turn_id], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            if events.is_empty() {
                continue;
            }
            let mut total_bytes = 0usize;
            for (_, _, bytes) in &events {
                total_bytes += *bytes as usize;
            }
            if events.len() as i64 <= MAX_TURN_EVENTS && total_bytes <= MAX_TURN_EVENT_BYTES {
                continue;
            }
            let is_essential = |kind: &str| {
                kind == "turn_started"
                    || kind == "error"
                    || matches!(kind, "done" | "turn_completed" | "cancelled" | "cancelling")
            };
            let mut essential_bytes = 0usize;
            let mut essential_count = 0usize;
            for (_, kind, bytes) in &events {
                if is_essential(kind) {
                    essential_count += 1;
                    essential_bytes += *bytes as usize;
                }
            }
            // 非 essential 事件的预算：扣掉 essential 后的剩余额度。
            // 从最新的普通事件开始保留（回放/诊断更关心靠近终态的上下文），
            // 最早的普通事件先被丢弃。
            let mut non_essential_budget =
                MAX_TURN_EVENTS as usize - essential_count.min(MAX_TURN_EVENTS as usize);
            let mut byte_budget = MAX_TURN_EVENT_BYTES.saturating_sub(essential_bytes);
            {
                let mut delete_stmt = tx.prepare(
                    "DELETE FROM turn_events WHERE session_id = ?1 AND turn_id = ?2 AND seq = ?3",
                )?;
                for (seq, kind, bytes) in events.iter().rev() {
                    if is_essential(kind) {
                        continue;
                    }
                    let fits = non_essential_budget > 0 && *bytes as usize <= byte_budget;
                    if fits {
                        non_essential_budget -= 1;
                        byte_budget = byte_budget.saturating_sub(*bytes as usize);
                    } else {
                        delete_stmt.execute(params![session_id, turn_id, seq])?;
                        pruned += 1;
                    }
                }
            }
        }
        tx.commit()?;
        Ok(pruned)
    }
}

fn read_turn_status(conn: &Connection, session_id: &str, turn_id: &str) -> Result<TurnState> {
    let status: String = conn.query_row(
        "SELECT status FROM turns WHERE session_id = ?1 AND turn_id = ?2",
        params![session_id, turn_id],
        |row| row.get(0),
    )?;
    TurnState::parse(&status)
        .ok_or_else(|| anyhow::anyhow!("turn '{}' 的状态 '{}' 非法，拒绝后续更新", turn_id, status))
}

fn read_turn(conn: &Connection, session_id: &str, turn_id: &str) -> Result<TurnRecord> {
    let mut stmt = conn.prepare(
        "SELECT session_id, turn_id, status, started_at, updated_at, ended_at, last_seq, cancellation_requested, detail, error_code
         FROM turns WHERE session_id = ?1 AND turn_id = ?2",
    )?;
    let record = stmt
        .query_row(params![session_id, turn_id], read_turn_row)
        .map_err(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => {
                anyhow::anyhow!("turn '{turn_id}' for session '{session_id}' was not found")
            }
            other => other.into(),
        })?;
    Ok(record)
}

fn read_turn_row(row: &rusqlite::Row<'_>) -> Result<TurnRecord, rusqlite::Error> {
    let status: String = row.get(2)?;
    let state = TurnState::parse(&status).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            anyhow::anyhow!("turn 状态 '{status}' 非法").into(),
        )
    })?;
    Ok(TurnRecord {
        session_id: row.get(0)?,
        turn_id: row.get(1)?,
        status: state,
        started_at: parse_rfc3339_strict(row.get::<_, String>(3)?)
            .map_err(|err| rusqlite_corruption(3, err))?,
        updated_at: parse_rfc3339_strict(row.get::<_, String>(4)?)
            .map_err(|err| rusqlite_corruption(4, err))?,
        ended_at: row
            .get::<_, Option<String>>(5)?
            .map(|value| parse_rfc3339_strict(value).map_err(|err| rusqlite_corruption(5, err)))
            .transpose()?,
        last_seq: row.get(6)?,
        cancellation_requested: row.get::<_, i64>(7)? != 0,
        detail: row.get(8)?,
        error_code: row.get(9)?,
    })
}

fn read_turn_event_row(row: &rusqlite::Row<'_>) -> Result<TurnEvent, rusqlite::Error> {
    Ok(TurnEvent {
        session_id: row.get(0)?,
        turn_id: row.get(1)?,
        seq: row.get(2)?,
        kind: row.get(3)?,
        timestamp: parse_rfc3339_strict(row.get::<_, String>(4)?)
            .map_err(|err| rusqlite_corruption(4, err))?,
        payload_json: row.get(5)?,
    })
}

/// 对事件 payload 落盘前统一脱敏：
/// - 疑似密钥字段（key/token/secret/authorization/bearer/password/credential 等）替换为掩码；
/// - 完整图片 base64（data:image/... 或长 base64 字符串）替换为掩码；
/// - 环境变量值中的密钥同样处理。
pub fn redact_event_payload(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (key, nested) in map {
                let is_secret_value = nested.is_string()
                    && looks_like_secret_string(nested.as_str().unwrap_or_default());
                if is_secret_key(key) || is_secret_value {
                    out.insert(key.clone(), Value::String(REDACTED.to_string()));
                } else {
                    out.insert(key.clone(), redact_event_payload(nested));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| {
                    if item.is_string()
                        && looks_like_secret_string(item.as_str().unwrap_or_default())
                    {
                        Value::String(REDACTED.to_string())
                    } else {
                        redact_event_payload(item)
                    }
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// 落盘前把原始 payload JSON 字符串脱敏为安全 JSON。
pub fn redact_event_payload_json(payload_json: &str) -> Result<String> {
    match serde_json::from_str::<Value>(payload_json) {
        Ok(value) => Ok(redact_event_payload(&value).to_string()),
        Err(_) => Ok(payload_json.to_string()),
    }
}

const REDACTED: &str = "[REDACTED]";

fn is_secret_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("api-key")
        || lower == "token"
        || lower.ends_with("_token")
        || lower.ends_with("-token")
        || lower.contains("access_token")
        || lower.contains("refresh_token")
        || lower.contains("id_token")
        || lower.contains("client_secret")
        || lower.contains("secret")
        || lower == "password"
        || lower.contains("authorization")
        || lower.contains("bearer")
        || lower.contains("credential")
        || lower.contains("session_key")
        || lower.contains("private_key")
}

fn looks_like_secret_string(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.starts_with("data:image/") {
        return true;
    }
    if trimmed.starts_with("Bearer ") || trimmed.starts_with("bearer ") {
        return true;
    }
    if trimmed.starts_with("sk-") || trimmed.starts_with("sk_") {
        return true;
    }
    if trimmed.starts_with("ghp_") || trimmed.starts_with("glpat-") {
        return true;
    }
    // 疑似完整图片 base64 / 长密钥：只含 base64 字符集、长度超过 256 的字符串
    if trimmed.len() > 256 {
        let is_base64ish = trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='));
        if is_base64ish {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn turn_state_parses_known_values_and_rejects_unknown() {
        for (text, expected) in [
            ("starting", TurnState::Starting),
            ("running", TurnState::Running),
            ("waiting_approval", TurnState::WaitingApproval),
            ("waiting_user", TurnState::WaitingUser),
            ("cancelling", TurnState::Cancelling),
            ("completed", TurnState::Completed),
            ("cancelled", TurnState::Cancelled),
            ("failed", TurnState::Failed),
            ("interrupted", TurnState::Interrupted),
        ] {
            assert_eq!(TurnState::parse(text), Some(expected));
            assert_eq!(expected.as_str(), text);
        }
        assert_eq!(TurnState::parse("banana"), None);
        assert_eq!(TurnState::parse("done"), None);
    }

    #[test]
    fn redaction_masks_secrets_and_image_base64() {
        let payload = json!({
            "title": "工具调用",
            "body": "运行命令",
            "env": { "OPENAI_API_KEY": "sk-test-1234567890abcdef" },
            "apiKey": "sk-live-abcdef",
            "authorization": "Bearer eyJhbGciOiJIUzI1NiJ9.xyz",
            "password": "hunter2",
            "image": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAAB",
            "metadata": { "token": "abcd" },
            "path": "/home/user/file.txt",
            "count": 42,
            "list": ["safe", "data:image/jpeg;base64,AAAA"]
        });
        let redacted = redact_event_payload(&payload);
        assert_eq!(redacted["apiKey"], "[REDACTED]");
        assert_eq!(redacted["authorization"], "[REDACTED]");
        assert_eq!(redacted["password"], "[REDACTED]");
        assert_eq!(redacted["image"], "[REDACTED]");
        assert_eq!(redacted["env"]["OPENAI_API_KEY"], "[REDACTED]");
        assert_eq!(redacted["metadata"]["token"], "[REDACTED]");
        assert_eq!(redacted["list"][1], "[REDACTED]");
        assert_eq!(redacted["path"], "/home/user/file.txt");
        assert_eq!(redacted["count"], 42);
        assert_eq!(redacted["list"][0], "safe");
        assert!(redacted.to_string().contains("[REDACTED]"));
        assert!(!redacted.to_string().contains("sk-live"));
        assert!(!redacted.to_string().contains("hunter2"));
    }
}
