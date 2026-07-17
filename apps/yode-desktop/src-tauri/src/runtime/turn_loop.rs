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
use super::PendingConfirmation;
use crate::protocol::DesktopEvent;

type TurnKey = (String, String);
type ConfirmSenderMap = Arc<
    Mutex<HashMap<TurnKey, tokio::sync::mpsc::UnboundedSender<yode_core::engine::ConfirmResponse>>>,
>;
type AskUserSenderMap = Arc<Mutex<HashMap<TurnKey, tokio::sync::mpsc::UnboundedSender<String>>>>;
type CancelTokenMap = Arc<Mutex<HashMap<TurnKey, tokio_util::sync::CancellationToken>>>;
type PendingConfirmationMap = Arc<Mutex<HashMap<TurnKey, PendingConfirmation>>>;

/// Drive the desktop turn event loop until the engine task finishes.
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
) {
    loop {
        let event = tokio::select! {
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
                seq += 1;
                continue;
            }
            Some(event) = event_rx.recv() => event,
            else => break,
        };

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

    if let Err(err) = handle.await {
        tracing::error!("Desktop turn task join failed: {}", err);
    }

    if let Ok(mut txs) = confirm_txs.lock() {
        txs.remove(&(session_id.clone(), turn_id.clone()));
    }
    if let Ok(mut txs) = ask_user_txs.lock() {
        txs.remove(&(session_id.clone(), turn_id.clone()));
    }
    if let Ok(mut tokens) = cancel_tokens.lock() {
        let _: Option<tokio_util::sync::CancellationToken> =
            tokens.remove(&(session_id.clone(), turn_id.clone()));
        if tokens.is_empty() {
            drop(tokens);
            stop_sleep_guard(&sleep_guard);
        }
    }
    if let Ok(mut pending) = pending_confirmations.lock() {
        pending.remove(&(session_id.clone(), turn_id.clone()));
    }
}
