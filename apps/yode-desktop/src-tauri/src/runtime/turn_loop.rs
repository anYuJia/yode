use std::collections::HashMap;
use std::process::Child;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use serde_json::json;
use tauri::AppHandle;
use tokio::sync::mpsc::UnboundedReceiver;
use yode_core::engine::EngineEvent;
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

/// Drive the desktop turn event loop until the engine task finishes.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_desktop_turn_event_loop(
    app: AppHandle,
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
                emit_desktop_event(
                    &app,
                    DesktopEvent {
                        session_id: session_id.clone(),
                        turn_id: turn_id.clone(),
                        seq,
                        kind: "cancelling".to_string(),
                        timestamp: Utc::now().to_rfc3339(),
                        payload: json!({ "title": "正在取消", "body": "正在停止本轮运行。" }),
                    },
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
                let desktop_event = DesktopEvent {
                    session_id: session_id.clone(),
                    turn_id: turn_id.clone(),
                    seq,
                    kind: "ask_user".to_string(),
                    timestamp: Utc::now().to_rfc3339(),
                    payload: json!({
                        "id": query.id,
                        "title": first_question.map(|question| question.header.clone()).unwrap_or_else(|| "需要用户输入".to_string()),
                        "body": first_question.map(|question| question.question.clone()).unwrap_or_else(|| "请在输入框回复。".to_string()),
                        "query": query
                    }),
                };
                emit_desktop_event(&app, desktop_event);
                update_run_state(&run_registry, &session_id, &turn_id, "waiting_user", None);
                seq += 1;
                continue;
            }
            Some(event) = event_rx.recv() => {
                let mapped = yode_runtime::engine_event_to_runtime_parts(event);
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
                let payload = mapped.payload;

                let next_status = match kind {
                    "tool_confirm_required" | "permission" => Some("waiting_approval"),
                    "error" => Some("failed"),
                    "turn_completed" | "done" => Some("completed"),
                    "turn_started" | "tool_started" | "assistant_text_delta" | "assistant_reasoning_delta" => Some("running"),
                    _ => None,
                };
                if let Some(status) = next_status {
                    let detail = (status == "failed")
                        .then(|| payload.get("body").and_then(|value| value.as_str()).map(str::to_string))
                        .flatten();
                    update_run_state(&run_registry, &session_id, &turn_id, status, detail);
                }

                if std::env::var("YODE_ACTION_NARRATIVE_DEBUG").is_ok_and(|value| value == "1")
                    && matches!(
                        kind,
                        "assistant_text_delta"
                            | "assistant_reasoning_delta"
                            | "action_narrative"
                            | "tool_started"
                            | "assistant_text_complete"
                            | "assistant_reasoning_complete"
                            | "turn_completed"
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
                        turn_id, kind, preview
                    );
                }

                let desktop_event = DesktopEvent {
                    session_id: session_id.clone(),
                    turn_id: turn_id.clone(),
                    seq,
                    kind: kind.to_string(),
                    timestamp: Utc::now().to_rfc3339(),
                    payload,
                };

                emit_desktop_event(&app, desktop_event);
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
        emit_desktop_event(
            &app,
            DesktopEvent {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                seq,
                kind: "cancelled".to_string(),
                timestamp: Utc::now().to_rfc3339(),
                payload: json!({ "title": "已取消", "body": "本轮运行已停止。" }),
            },
        );
        update_run_state(&run_registry, &session_id, &turn_id, "cancelled", None);
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
