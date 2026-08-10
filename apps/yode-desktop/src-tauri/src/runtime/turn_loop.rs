use std::collections::HashMap;
use std::process::Child;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use tauri::AppHandle;
use tokio::sync::mpsc::UnboundedReceiver;
use yode_core::db::{Database, TurnEvent, TurnState};
use yode_core::engine::EngineEvent;
use yode_runtime::{
    engine_event_to_runtime_parts, run_status_for_event_kind, DesktopEventEnvelope,
    DesktopEventKind, DesktopEventPayload,
};
use yode_tools::tool::UserQuery;

use super::settings_system::stop_sleep_guard;
use super::turn_events::emit_desktop_event;
use super::turn_runtime::{release_turn_occupancy, update_run_state, SessionOperationMap};
use super::PendingConfirmation;
use crate::protocol::DesktopEvent;
use crate::protocol::SessionRunState;

type TurnKey = (String, String);
type ConfirmSenderMap = Arc<
    Mutex<HashMap<TurnKey, tokio::sync::mpsc::UnboundedSender<yode_core::engine::ConfirmResponse>>>,
>;
type AskUserSenderMap = Arc<Mutex<HashMap<TurnKey, tokio::sync::mpsc::UnboundedSender<String>>>>;
pub(super) type CancelTokenMap = Arc<Mutex<HashMap<TurnKey, tokio_util::sync::CancellationToken>>>;
type PendingConfirmationMap = Arc<Mutex<HashMap<TurnKey, PendingConfirmation>>>;

/// 事件信封 → 前端可见事件：经 `DesktopEventEnvelope::new` 统一构造
/// （强类型 kind + 稳定 schemaVersion），再平铺为线上线型。
/// payload 落盘前由 DB 层统一脱敏。
pub(super) fn envelope_to_desktop_event(
    session_id: &str,
    turn_id: &str,
    seq: u64,
    timestamp: String,
    kind: DesktopEventKind,
    payload: serde_json::Value,
) -> DesktopEvent {
    DesktopEventEnvelope::new(session_id, turn_id, seq, timestamp, kind, payload).into()
}

/// 在事件写入的同一临界区内同步持久化 turn 状态（数据库为事实来源，
/// run_registry 仅作内存热缓存）。事件内容、seq 与生命周期状态在同一个
/// 事务内落盘；失败只记录诊断，不中断事件流。
#[allow(clippy::too_many_arguments)]
fn persist_event(
    db: &Database,
    run_registry: &Arc<Mutex<HashMap<String, SessionRunState>>>,
    session_id: &str,
    turn_id: &str,
    seq: u64,
    timestamp: chrono::DateTime<Utc>,
    kind: DesktopEventKind,
    payload: &serde_json::Value,
) {
    let detail = (kind == DesktopEventKind::Error)
        .then(|| {
            payload
                .get("body")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .flatten();
    let (state, error_code) = match kind {
        DesktopEventKind::AskUser => (Some(TurnState::WaitingUser), None),
        _ => match run_status_for_event_kind(kind) {
            Some("waiting_approval") => (Some(TurnState::WaitingApproval), None),
            Some("failed") => (Some(TurnState::Failed), Some("run_failed".to_string())),
            Some("completed") => (Some(TurnState::Completed), None),
            Some("cancelled") => (Some(TurnState::Cancelled), None),
            Some("cancelling") => (Some(TurnState::Cancelling), None),
            Some("running") => (Some(TurnState::Running), None),
            _ => (None, None),
        },
    };
    if let Err(err) = db.append_turn_event_with_state(
        &TurnEvent {
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            seq: seq as i64,
            kind: kind.as_str().to_string(),
            timestamp,
            payload_json: payload.to_string(),
        },
        state,
        detail.clone(),
        error_code,
    ) {
        tracing::error!(
            session_id = %session_id,
            turn_id = %turn_id,
            kind = %kind.as_str(),
            error = %err,
            "Failed to persist turn event"
        );
    }
    if let Some(state) = state {
        update_run_state(run_registry, session_id, turn_id, state.as_str(), detail);
    }
}

/// 终态 turn 的 journal 已完整落盘，可安全触发限量清理（不影响其他运行中 turn）。
fn prune_after_terminal(db: &Database) {
    if let Err(err) = db.prune_turn_journals() {
        tracing::error!("Failed to prune turn journals: {}", err);
    }
}

/// Drive the desktop turn event loop until the engine task finishes.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_desktop_turn_event_loop(
    app: AppHandle,
    db: Database,
    session_id: String,
    turn_id: String,
    mut seq: u64,
    mut event_rx: UnboundedReceiver<EngineEvent>,
    mut ask_user_query_rx: UnboundedReceiver<UserQuery>,
    handle: tokio::task::JoinHandle<()>,
    confirm_txs: ConfirmSenderMap,
    ask_user_txs: AskUserSenderMap,
    cancel_tokens: CancelTokenMap,
    pending_confirmations: PendingConfirmationMap,
    sleep_guard: Arc<Mutex<Option<Child>>>,
    cancel_token: tokio_util::sync::CancellationToken,
    active_sessions: SessionOperationMap,
    run_registry: Arc<Mutex<HashMap<String, SessionRunState>>>,
) {
    let mut cancelled = false;
    loop {
        tokio::select! {
            // 取消：等待引擎真正停止（channel 关闭即 quiesce），期间丢弃所有迟到事件，
            // turn task 完全退出后才会释放 in-flight 占位并发出 cancelled 终态。
            // 占位在 quiesce 之后释放：取消请求期间新 turn 一律被拒绝，
            // 前端收到 cancelled 时旧工具已停止，可安全发起新 turn。
            _ = cancel_token.cancelled() => {
                cancelled = true;
                let now = Utc::now();
                let timestamp = now.to_rfc3339();
                let payload = DesktopEventPayload::Cancelling(
                    yode_runtime::CancellingPayload {
                        title: "正在取消",
                        body: "正在停止本轮运行。",
                    }
                ).as_value();
                if let Err(err) = db.mark_turn_cancellation_requested(&session_id, &turn_id) {
                    tracing::error!("Failed to persist cancellation request: {}", err);
                }
                persist_event(
                    &db,
                    &run_registry,
                    &session_id,
                    &turn_id,
                    seq,
                    now,
                    DesktopEventKind::Cancelling,
                    &payload,
                );
                emit_desktop_event(
                    &app,
                    envelope_to_desktop_event(
                        &session_id,
                        &turn_id,
                        seq,
                        timestamp,
                        DesktopEventKind::Cancelling,
                        payload,
                    ),
                );
                seq += 1;
                let drain = async {
                    while event_rx.recv().await.is_some() {}
                    while ask_user_query_rx.recv().await.is_some() {}
                };
                if tokio::time::timeout(std::time::Duration::from_secs(10), drain)
                    .await
                    .is_err()
                {
                    handle.abort();
                }
                break;
            }
            Some(query) = ask_user_query_rx.recv() => {
                let first_question = query.questions.first();
                let payload = DesktopEventPayload::AskUser(yode_runtime::AskUserPayload {
                    id: query.id.clone(),
                    title: first_question.map(|question| question.header.clone()).unwrap_or_else(|| "需要用户输入".to_string()),
                    body: first_question.map(|question| question.question.clone()).unwrap_or_else(|| "请在输入框回复。".to_string()),
                    tool: "ask_user",
                    meta: "等待用户回答",
                    query: serde_json::to_value(&query).ok(),
                })
                .as_value();
                persist_event(
                    &db,
                    &run_registry,
                    &session_id,
                    &turn_id,
                    seq,
                    Utc::now(),
                    DesktopEventKind::AskUser,
                    &payload,
                );
                emit_desktop_event(
                    &app,
                    envelope_to_desktop_event(
                        &session_id,
                        &turn_id,
                        seq,
                        Utc::now().to_rfc3339(),
                        DesktopEventKind::AskUser,
                        payload,
                    ),
                );
                seq += 1;
                continue;
            }
            Some(event) = event_rx.recv() => {
                let mapped = engine_event_to_runtime_parts(event);
                if let Some(pending_confirmation) = mapped.pending_confirmation.as_ref() {
                    if let Ok(mut pending) = pending_confirmations.lock() {
                        pending.insert(
                            (session_id.clone(), turn_id.clone()),
                            PendingConfirmation {
                                tool_name: pending_confirmation.tool_name.clone(),
                                command: pending_confirmation.command.clone(),
                            },
                        );
                    }
                }

                let kind = mapped.kind;
                let payload = mapped.payload.as_value();
                let timestamp = Utc::now();
                persist_event(
                    &db,
                    &run_registry,
                    &session_id,
                    &turn_id,
                    seq,
                    timestamp,
                    kind,
                    &payload,
                );

                if std::env::var("YODE_ACTION_NARRATIVE_DEBUG").is_ok_and(|value| value == "1")
                    && matches!(
                        kind,
                        DesktopEventKind::AssistantTextDelta
                            | DesktopEventKind::AssistantReasoningDelta
                            | DesktopEventKind::ActionNarrative
                            | DesktopEventKind::ToolStarted
                            | DesktopEventKind::AssistantTextComplete
                            | DesktopEventKind::AssistantReasoningComplete
                            | DesktopEventKind::TurnCompleted
                    )
                {
                    let preview = payload
                        .get("body")
                        .or_else(|| payload.get("reasoning"))
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .chars()
                        .take(120)
                        .collect::<String>()
                        .replace('\n', "\\n");
                    eprintln!(
                        "[action-narrative-debug] turn={} kind={} preview={:?}",
                        turn_id,
                        kind.as_str(),
                        preview
                    );
                }

                emit_desktop_event(
                    &app,
                    envelope_to_desktop_event(
                        &session_id,
                        &turn_id,
                        seq,
                        timestamp.to_rfc3339(),
                        kind,
                        payload,
                    ),
                );
                seq += 1;
            }
            else => break,
        }
    }
    // 即使 drain 超时并调用了 abort，也必须等待 task 的 JoinHandle 真正结束。
    // 在此之前保留 turn 槽位，防止 clear/delete/compact 或新 turn 与旧任务并发。
    let no_active_turns = join_turn_then_release_occupancy(
        handle,
        &active_sessions,
        &cancel_tokens,
        &session_id,
        &turn_id,
    )
    .await;

    if let Ok(mut txs) = confirm_txs.lock() {
        txs.remove(&(session_id.clone(), turn_id.clone()));
    }
    if let Ok(mut txs) = ask_user_txs.lock() {
        txs.remove(&(session_id.clone(), turn_id.clone()));
    }
    if no_active_turns {
        stop_sleep_guard(&sleep_guard);
    }
    if let Ok(mut pending) = pending_confirmations.lock() {
        pending.remove(&(session_id.clone(), turn_id.clone()));
    }
    if cancelled {
        let timestamp = Utc::now();
        let payload = DesktopEventPayload::Cancelled(yode_runtime::CancelledPayload {
            title: "已取消",
            body: "本轮运行已停止。",
        })
        .as_value();
        persist_event(
            &db,
            &run_registry,
            &session_id,
            &turn_id,
            seq,
            timestamp,
            DesktopEventKind::Cancelled,
            &payload,
        );
        emit_desktop_event(
            &app,
            envelope_to_desktop_event(
                &session_id,
                &turn_id,
                seq,
                timestamp.to_rfc3339(),
                DesktopEventKind::Cancelled,
                payload,
            ),
        );
        prune_after_terminal(&db);
    } else {
        // 事件流自然关闭（无取消）：终态已由 persist_event 落盘，
        // 这里仅确认 DB 中确实存在终态；不存在时补一个 interrupted 诊断。
        if let Ok(Some(turn)) = db.get_turn(&session_id, &turn_id) {
            if !turn.status.is_terminal() {
                let _ = db.update_turn_state(
                    &session_id,
                    &turn_id,
                    TurnState::Interrupted,
                    Some("事件流意外终止，未收到终态事件。".to_string()),
                    Some("stream_closed_without_terminal".to_string()),
                );
            }
        }
    }
}

/// 等待 turn task 完全退出后才释放对应的生命周期槽位。
///
/// 该函数在取消、错误和正常完成路径中共用，避免 `abort()` 仅提交取消请求时就
/// 允许同一会话的破坏性操作进入。
pub(super) async fn join_turn_then_release_occupancy(
    handle: tokio::task::JoinHandle<()>,
    active_sessions: &SessionOperationMap,
    cancel_tokens: &CancelTokenMap,
    session_id: &str,
    turn_id: &str,
) -> bool {
    if let Err(err) = handle.await {
        tracing::error!("Desktop turn task join failed: {}", err);
    }
    release_turn_occupancy(active_sessions, cancel_tokens, session_id, turn_id);
    cancel_tokens
        .lock()
        .map(|tokens| tokens.is_empty())
        .unwrap_or(false)
}
