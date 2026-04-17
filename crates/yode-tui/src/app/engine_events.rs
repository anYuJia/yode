pub(in crate::app) mod provider;
mod streaming;

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{mpsc, Mutex};

use yode_core::engine::{AgentEngine, ConfirmResponse, EngineEvent};

use crate::runtime_display::{
    format_budget_exceeded_message, format_context_compressed_message,
    format_session_memory_update_message,
};
use self::streaming::{
    finalize_streaming, handle_done, handle_reasoning_complete, handle_reasoning_delta,
    handle_suggestion_ready, handle_text_complete, handle_text_delta, handle_turn_complete,
};
use super::{
    push_system_entry, App, ChatEntry, ChatRole, PendingConfirmation, PermissionMode,
    TurnStatus,
};

pub(super) fn handle_engine_event(
    app: &mut App,
    event: EngineEvent,
    engine: &Arc<Mutex<AgentEngine>>,
    engine_event_tx: &mpsc::UnboundedSender<EngineEvent>,
) {
    match event {
        EngineEvent::Thinking => resume_working_after_retry(&mut app.turn_status),
        EngineEvent::UsageUpdate(usage) => {
            if usage.prompt_tokens > 0 {
                let new_tokens = if usage.prompt_tokens > app.session.previous_prompt_tokens {
                    usage.prompt_tokens - app.session.previous_prompt_tokens
                } else {
                    usage.prompt_tokens
                };
                if new_tokens > 0 {
                    app.session.turn_input_tokens = new_tokens;
                }
            }
            if usage.completion_tokens > 0 {
                app.session.turn_output_tokens = usage.completion_tokens;
            }
        }
        EngineEvent::TextDelta(delta) => {
            resume_working_after_retry(&mut app.turn_status);
            handle_text_delta(app, delta)
        }
        EngineEvent::ReasoningDelta(delta) => {
            resume_working_after_retry(&mut app.turn_status);
            handle_reasoning_delta(app, delta)
        }
        EngineEvent::TextComplete(text) => handle_text_complete(app, text),
        EngineEvent::ReasoningComplete(text) => handle_reasoning_complete(app, text),
        EngineEvent::ToolCallStart {
            id,
            name,
            arguments,
        } => {
            resume_working_after_retry(&mut app.turn_status);
            finalize_streaming(app);
            app.turn_tool_count += 1;

            if app.in_sub_agent {
                app.sub_agent_tool_count += 1;
                app.session.tool_call_count += 1;
                app.tool_call_starts.insert(id, Instant::now());
                app.chat_entries.push(ChatEntry::new(
                    ChatRole::SubAgentToolCall { name },
                    arguments,
                ));
                return;
            }

            let existing =
                app.chat_entries.iter_mut().rev().take(10).find(
                    |e| matches!(&e.role, ChatRole::ToolCall { id: ref eid, .. } if eid == &id),
                );

            if let Some(entry) = existing {
                entry.content = arguments;
            } else {
                app.session.tool_call_count += 1;
                app.tool_call_starts.insert(id.clone(), Instant::now());
                app.chat_entries
                    .push(ChatEntry::new(ChatRole::ToolCall { id, name }, arguments));
            }
        }
        EngineEvent::ToolConfirmRequired {
            id,
            name,
            arguments,
        } => {
            let existing =
                app.chat_entries.iter_mut().rev().take(10).find(
                    |e| matches!(&e.role, ChatRole::ToolCall { id: ref eid, .. } if eid == &id),
                );

            if let Some(entry) = existing {
                if entry.content.is_empty() {
                    entry.content = arguments.clone();
                }
            } else {
                app.chat_entries.push(ChatEntry::new(
                    ChatRole::ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                    },
                    arguments.clone(),
                ));
            }

            if app.session.permission_mode == PermissionMode::AutoAccept
                || app.session.always_allow_tools.contains(&name)
            {
                if let Some(tx) = &app.confirm_tx {
                    let _ = tx.send(ConfirmResponse::Allow);
                }
            } else {
                app.pending_confirmation = Some(PendingConfirmation {
                    id,
                    name,
                    arguments,
                });
                app.confirm_selected = 0;
            }
        }
        EngineEvent::ToolProgress {
            id,
            name: _,
            progress,
        } => {
            resume_working_after_retry(&mut app.turn_status);
            let existing =
                app.chat_entries.iter_mut().rev().take(15).find(
                    |e| matches!(&e.role, ChatRole::ToolCall { id: ref eid, .. } if eid == &id),
                );
            if let Some(entry) = existing {
                entry.progress = Some(progress);
            }
        }
        EngineEvent::ToolResult { id, name, result } => {
            resume_working_after_retry(&mut app.turn_status);
            let duration = app
                .tool_call_starts
                .remove(&id)
                .map(|start| start.elapsed());
            let mut entry = ChatEntry::new(
                ChatRole::ToolResult {
                    id,
                    name,
                    is_error: result.is_error,
                },
                result.content,
            );
            entry.duration = duration;
            entry.tool_metadata = result.metadata.clone();
            entry.tool_error_type = result.error_type.map(|kind| format!("{:?}", kind));
            app.chat_entries.push(entry);
        }
        EngineEvent::TurnComplete(response) => handle_turn_complete(app, response),
        EngineEvent::Error(e) => {
            finalize_streaming(app);
            app.thinking_printed = false;
            app.chat_entries.push(ChatEntry::new(ChatRole::Error, e));
        }
        EngineEvent::Retrying {
            error_message,
            attempt,
            max_attempts,
            delay_secs,
        } => {
            finalize_streaming(app);
            app.thinking_printed = false;
            app.turn_status = TurnStatus::Retrying {
                verb: retry_verb(&app.turn_status),
                error: error_message,
                attempt,
                max_attempts,
                delay_secs,
            };
        }
        EngineEvent::AskUser { id, question } => {
            finalize_streaming(app);
            app.chat_entries.push(ChatEntry::new(
                ChatRole::AskUser { id },
                format!("❓ {}", question),
            ));
        }
        EngineEvent::Done => handle_done(app, engine, engine_event_tx),
        EngineEvent::SuggestionReady { suggestion } => handle_suggestion_ready(app, suggestion),
        EngineEvent::SessionMemoryUpdated {
            path,
            generated_summary,
        } => {
            push_system_entry(app, format_session_memory_update_message(&path, generated_summary));
        }
        EngineEvent::UpdateAvailable(version) => {
            app.update_available = Some(version);
        }
        EngineEvent::UpdateDownloading => {
            app.update_downloading = true;
        }
        EngineEvent::UpdateDownloaded(version) => {
            app.update_downloading = false;
            app.update_downloaded = Some(version);
            app.update_available = None;
        }
        EngineEvent::SubAgentStart { description } => {
            finalize_streaming(app);
            app.in_sub_agent = true;
            app.sub_agent_tool_count = 0;
            app.chat_entries.push(ChatEntry::new(
                ChatRole::SubAgentCall { description },
                String::new(),
            ));
        }
        EngineEvent::SubAgentComplete { result } => {
            app.in_sub_agent = false;
            let mut entry = ChatEntry::new(ChatRole::SubAgentResult, result);
            if let Some(call_entry) = app
                .chat_entries
                .iter()
                .rev()
                .find(|e| matches!(&e.role, ChatRole::SubAgentCall { .. }))
            {
                entry.duration = Some(call_entry.timestamp.elapsed());
            }
            app.chat_entries.push(entry);
        }
        EngineEvent::PlanModeEntered => {
            push_system_entry(app, "📋 Entered plan mode (read-only tools only)");
        }
        EngineEvent::PlanApprovalRequired { plan_content } => {
            let preview = if plan_content.len() > 500 {
                format!("{}...", &plan_content[..500])
            } else {
                plan_content
            };
            push_system_entry(app, format!("📋 Plan ready for approval:\n{}", preview));
        }
        EngineEvent::PlanModeExited => {
            push_system_entry(app, "📋 Exited plan mode");
        }
        EngineEvent::ContextCompressed {
            mode,
            removed,
            tool_results_truncated,
            summary,
            session_memory_path,
            transcript_path,
        } => {
            push_system_entry(
                app,
                format_context_compressed_message(
                    &mode,
                    removed,
                    tool_results_truncated,
                    summary.as_deref(),
                    session_memory_path.as_deref(),
                    transcript_path.as_deref(),
                ),
            );
        }
        EngineEvent::CostUpdate {
            estimated_cost,
            input_tokens,
            output_tokens,
            cache_write_tokens,
            cache_read_tokens,
        } => {
            tracing::debug!(
                "Cost: ${:.4} ({}in/{}out, {} cache_write/{} cache_read)",
                estimated_cost,
                input_tokens,
                output_tokens,
                cache_write_tokens,
                cache_read_tokens
            );
        }
        EngineEvent::BudgetExceeded { cost, limit } => {
            push_system_entry(app, format_budget_exceeded_message(cost, limit));
        }
    }
}

fn retry_verb(status: &TurnStatus) -> &'static str {
    match status {
        TurnStatus::Working { verb } | TurnStatus::Retrying { verb, .. } => *verb,
        _ => "Working",
    }
}

fn resume_working_after_retry(status: &mut TurnStatus) {
    if let TurnStatus::Retrying { verb, .. } = *status {
        *status = TurnStatus::Working { verb };
    }
}

#[cfg(test)]
mod tests {
    use super::{resume_working_after_retry, retry_verb};
    use crate::app::TurnStatus;

    #[test]
    fn retry_status_preserves_original_verb() {
        let status = TurnStatus::Working { verb: "Brewing" };
        assert_eq!(retry_verb(&status), "Brewing");

        let status = TurnStatus::Retrying {
            verb: "Weaving",
            error: "network".to_string(),
            attempt: 2,
            max_attempts: 10,
            delay_secs: 0,
        };
        assert_eq!(retry_verb(&status), "Weaving");
    }

    #[test]
    fn retry_status_recovers_to_working_on_progress() {
        let mut status = TurnStatus::Retrying {
            verb: "Forging",
            error: "network".to_string(),
            attempt: 3,
            max_attempts: 10,
            delay_secs: 0,
        };
        resume_working_after_retry(&mut status);
        match status {
            TurnStatus::Working { verb } => assert_eq!(verb, "Forging"),
            other => panic!("expected working status, got {:?}", other),
        }
    }
}
