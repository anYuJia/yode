pub(in crate::app) mod provider;
mod streaming;

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{mpsc, Mutex};

use yode_core::engine::{AgentEngine, ConfirmResponse, EngineEvent};

use self::streaming::{
    finalize_streaming, handle_done, handle_reasoning_complete, handle_reasoning_delta,
    handle_suggestion_ready, handle_text_complete, handle_text_delta, handle_turn_complete,
};
use super::{
    push_grouped_system_entry, App, ChatEntry, ChatRole, PendingConfirmation, PermissionMode,
    TurnStatus,
};

pub(super) fn handle_engine_event(
    app: &mut App,
    event: EngineEvent,
    engine: &Arc<Mutex<AgentEngine>>,
    engine_event_tx: &mpsc::UnboundedSender<EngineEvent>,
) {
    match event {
        EngineEvent::Thinking => {}
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
        EngineEvent::TextDelta(delta) => handle_text_delta(app, delta),
        EngineEvent::ReasoningDelta(delta) => handle_reasoning_delta(app, delta),
        EngineEvent::TextComplete(text) => handle_text_complete(app, text),
        EngineEvent::ReasoningComplete(text) => handle_reasoning_complete(app, text),
        EngineEvent::ToolCallStart {
            id,
            name,
            arguments,
        } => {
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

            let existing = app.chat_entries.iter_mut().rev().take(10).find(
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
            let existing = app.chat_entries.iter_mut().rev().take(10).find(
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
        EngineEvent::ToolProgress { id, name: _, progress } => {
            let existing = app.chat_entries.iter_mut().rev().take(15).find(
                |e| matches!(&e.role, ChatRole::ToolCall { id: ref eid, .. } if eid == &id),
            );
            if let Some(entry) = existing {
                entry.progress = Some(progress);
            }
        }
        EngineEvent::ToolResult { id, name, result } => {
            let duration = app.tool_call_starts.remove(&id).map(|start| start.elapsed());
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
            push_grouped_system_entry(
                app,
                "Session memory updated",
                format!(
                    "Session memory updated ({}): {}",
                    if generated_summary {
                        "summary"
                    } else {
                        "snapshot"
                    },
                    path
                ),
            );
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
            app.chat_entries.push(ChatEntry::new(
                ChatRole::System,
                "📋 Entered plan mode (read-only tools only)".to_string(),
            ));
        }
        EngineEvent::PlanApprovalRequired { plan_content } => {
            let preview = if plan_content.len() > 500 {
                format!("{}...", &plan_content[..500])
            } else {
                plan_content
            };
            app.chat_entries.push(ChatEntry::new(
                ChatRole::System,
                format!("📋 Plan ready for approval:\n{}", preview),
            ));
        }
        EngineEvent::PlanModeExited => {
            app.chat_entries.push(ChatEntry::new(
                ChatRole::System,
                "📋 Exited plan mode".to_string(),
            ));
        }
        EngineEvent::ContextCompressed {
            mode,
            removed,
            tool_results_truncated,
            summary,
            session_memory_path,
            transcript_path,
        } => {
            let mut content = match (removed, tool_results_truncated) {
                (0, truncated) => format!(
                    "Context compressed ({}): truncated {} oversized tool results to stay within the window.",
                    mode, truncated
                ),
                (removed, 0) => format!(
                    "Context compressed ({}): removed {} messages to fit window.",
                    mode, removed
                ),
                (removed, truncated) => format!(
                    "Context compressed ({}): removed {} messages and truncated {} oversized tool results.",
                    mode, removed, truncated
                ),
            };

            if let Some(summary) = summary {
                content.push_str("\n");
                content.push_str(&summary);
            }

            if let Some(path) = session_memory_path {
                content.push_str("\nSession memory: ");
                content.push_str(&path);
            }

            if let Some(path) = transcript_path {
                content.push_str("\nTranscript backup: ");
                content.push_str(&path);
            }

            push_grouped_system_entry(app, "Context compressed", content);
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
            app.chat_entries.push(ChatEntry::new(
                ChatRole::System,
                format!(
                    "⚠ Budget limit exceeded: ${:.4} (limit: ${:.2})",
                    cost, limit
                ),
            ));
        }
    }
}
