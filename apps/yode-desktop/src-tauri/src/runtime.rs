use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::json;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;
use yode_core::db::Database;
use yode_core::session::Session;

use crate::protocol::{
    Bootstrap, CreateSessionRequest, DesktopEvent, DesktopSession, RuntimeState,
    SendMessageRequest, TurnAccepted,
};

pub struct DesktopRuntime {
    db: Database,
    workspace_path: PathBuf,
    provider: String,
    model: String,
    permission_mode: String,
    active_session_id: Mutex<Option<String>>,
    seq: AtomicU64,
}

impl DesktopRuntime {
    pub fn new() -> Result<Self> {
        let workspace_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let db_path = dirs::home_dir()
            .unwrap_or_else(|| workspace_path.clone())
            .join(".yode")
            .join("sessions.db");

        Ok(Self {
            db: Database::open(&db_path)?,
            workspace_path,
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4".to_string(),
            permission_mode: "Default".to_string(),
            active_session_id: Mutex::new(None),
            seq: AtomicU64::new(1),
        })
    }

    pub fn bootstrap(&self) -> Result<Bootstrap> {
        let sessions = self.sessions_list()?;
        Ok(Bootstrap {
            app_version: env!("CARGO_PKG_VERSION"),
            workspace_path: self.workspace_path.display().to_string(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            permission_mode: self.permission_mode.clone(),
            sessions,
        })
    }

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
        let session = Session {
            id: Uuid::new_v4().to_string(),
            name: request.title.or_else(|| Some("桌面端会话".to_string())),
            provider: request.provider.unwrap_or_else(|| self.provider.clone()),
            model: request.model.unwrap_or_else(|| self.model.clone()),
            created_at: now,
            updated_at: now,
        };

        self.db.create_session(&session)?;
        self.set_active_session(session.id.clone())?;
        Ok(self.map_session(session, None))
    }

    pub fn runtime_state(&self) -> Result<RuntimeState> {
        Ok(RuntimeState {
            active_session_id: self
                .active_session_id
                .lock()
                .map_err(|_| anyhow::anyhow!("active session lock poisoned"))?
                .clone(),
            status: "idle".to_string(),
            permission_mode: self.permission_mode.clone(),
            context_percent: 31,
            tool_calls: "0 / 0".to_string(),
        })
    }

    pub fn turn_send_message(
        &self,
        app: AppHandle,
        request: SendMessageRequest,
    ) -> Result<TurnAccepted> {
        let content = request.content.trim().to_string();
        if content.is_empty() {
            anyhow::bail!("message content cannot be empty");
        }

        let session = self
            .db
            .get_session(&request.session_id)?
            .with_context(|| format!("session '{}' not found", request.session_id))?;

        self.set_active_session(session.id.clone())?;
        self.db
            .save_message(&session.id, "user", Some(&content), None, None, None)?;
        self.db.touch_session(&session.id)?;

        let turn_id = Uuid::new_v4().to_string();
        let session_id = session.id.clone();
        let emit_turn_id = turn_id.clone();
        let seq_base = self.seq.fetch_add(10, Ordering::SeqCst);
        std::thread::spawn(move || {
            emit_mock_turn(app, session_id, emit_turn_id, seq_base, content);
        });

        Ok(TurnAccepted {
            session_id: session.id,
            turn_id,
        })
    }

    fn set_active_session(&self, session_id: String) -> Result<()> {
        *self
            .active_session_id
            .lock()
            .map_err(|_| anyhow::anyhow!("active session lock poisoned"))? = Some(session_id);
        Ok(())
    }

    fn map_session(&self, session: Session, active_session_id: Option<&str>) -> DesktopSession {
        DesktopSession {
            id: session.id.clone(),
            title: session
                .name
                .clone()
                .unwrap_or_else(|| session.id.chars().take(8).collect()),
            project: self
                .workspace_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("workspace")
                .to_string(),
            provider: session.provider,
            model: session.model,
            updated_at: relative_time(session.updated_at),
            active: active_session_id == Some(session.id.as_str()),
        }
    }
}

fn emit_mock_turn(
    app: AppHandle,
    session_id: String,
    turn_id: String,
    seq_base: u64,
    content: String,
) {
    let events = [
        (
            "turn_started",
            json!({
                "title": "开始处理",
                "body": content,
            }),
        ),
        (
            "assistant_reasoning_delta",
            json!({
                "title": "运行时规划",
                "body": "桌面端事件链路已建立。下一步可以把这里替换为真实 EngineEvent。",
                "meta": "desktop runtime",
            }),
        ),
        (
            "tool_started",
            json!({
                "title": "读取 runtime 状态",
                "body": "Tauri command 已进入 Rust runtime，并通过事件返回前端。",
                "tool": "runtime_state_get",
                "status": "running",
                "meta": "bridge",
            }),
        ),
        (
            "tool_result",
            json!({
                "title": "读取 runtime 状态",
                "body": "session、turn 和 desktop-event 通道已跑通。",
                "tool": "runtime_state_get",
                "status": "success",
                "meta": "ok",
            }),
        ),
        (
            "assistant_text_delta",
            json!({
                "title": "Yode",
                "body": "桌面端最小闭环成立：前端发送消息，Rust runtime 接收，后端再把结构化事件推回 UI。",
                "meta": "stream complete",
            }),
        ),
        (
            "turn_completed",
            json!({
                "status": "completed",
                "toolCalls": "1 / 1",
                "contextPercent": 31,
            }),
        ),
    ];

    for (offset, (kind, payload)) in events.into_iter().enumerate() {
        let event = DesktopEvent {
            session_id: session_id.clone(),
            turn_id: turn_id.clone(),
            seq: seq_base + offset as u64,
            kind: kind.to_string(),
            timestamp: Utc::now().to_rfc3339(),
            payload,
        };

        let _ = app.emit("desktop-event", event);
        std::thread::sleep(Duration::from_millis(180));
    }
}

fn relative_time(updated_at: DateTime<Utc>) -> String {
    let delta = Utc::now().signed_duration_since(updated_at);
    if delta.num_seconds() < 60 {
        "刚刚".to_string()
    } else if delta.num_minutes() < 60 {
        format!("{} 分钟前", delta.num_minutes())
    } else if delta.num_hours() < 24 {
        format!("{} 小时前", delta.num_hours())
    } else {
        updated_at.format("%m-%d").to_string()
    }
}
