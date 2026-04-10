use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{mpsc, Mutex};

use yode_core::engine::{AgentEngine, ConfirmResponse, EngineEvent};

use super::{
    find_case_insensitive, push_grouped_system_entry, strip_internal_tags, try_process_next, App,
    ChatEntry, ChatRole, PendingConfirmation, PermissionMode, TurnStatus, TAG_RE,
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
        EngineEvent::TextDelta(delta) => {
            app.streaming_tag_buf.push_str(&delta);

            if app.streaming_tag_buf.len() > 500 {
                app.streaming_buf
                    .push_str(&std::mem::take(&mut app.streaming_tag_buf));
            } else {
                let triggers = ["[tool_use", "[tool_result", "[DUMMY_TOOL", "name=bash"];
                let mut has_trigger = false;
                for &t in &triggers {
                    if let Some(pos) = app.streaming_tag_buf.find(t) {
                        if let Some(m) = TAG_RE.find(&app.streaming_tag_buf) {
                            let end = m.end();
                            let _tag_content = app.streaming_tag_buf[..end].to_string();
                            app.streaming_tag_buf = app.streaming_tag_buf[end..].to_string();
                        } else {
                            let after_trigger = &app.streaming_tag_buf[pos + t.len()..];
                            if after_trigger.trim().is_empty()
                                || after_trigger.contains('{')
                                || after_trigger.contains('i')
                            {
                                has_trigger = true;
                                break;
                            }
                        }
                    }
                }

                if !has_trigger {
                    app.streaming_buf
                        .push_str(&std::mem::take(&mut app.streaming_tag_buf));
                }
            }

            if app.streaming_buf.is_empty() {
                return;
            }

            let s_content = std::mem::take(&mut app.streaming_buf);
            let mut s = s_content.as_str();

            while let Some(start) = find_case_insensitive(s, "<thinking>") {
                if start > 0 {
                    app.streaming_buf.push_str(&s[..start]);
                }

                let tag_end = start + 10;
                if let Some(end) = find_case_insensitive(&s[tag_end..], "</thinking>") {
                    let thinking_content = &s[tag_end..tag_end + end];
                    app.streaming_reasoning.push_str(thinking_content);
                    s = &s[tag_end + end + 11..];
                } else {
                    app.streaming_reasoning.push_str(&s[tag_end..]);
                    s = "";
                }
            }
            if !s.is_empty() {
                app.streaming_buf.push_str(s);
            }
        }
        EngineEvent::ReasoningDelta(delta) => {
            app.received_reasoning_delta = true;
            app.streaming_reasoning.push_str(&delta);
        }
        EngineEvent::TextComplete(text) => {
            if app.streaming_buf.is_empty() {
                app.streaming_buf = text;
            }
        }
        EngineEvent::ReasoningComplete(text) => {
            if app.streaming_reasoning.is_empty() {
                app.streaming_reasoning = text;
            }
        }
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
        EngineEvent::TurnComplete(response) => {
            finalize_streaming(app);

            let prompt = response.usage.prompt_tokens;
            let completion = response.usage.completion_tokens;
            let total = response.usage.total_tokens;

            if prompt > 0 {
                let new_tokens = if prompt > app.session.previous_prompt_tokens {
                    prompt - app.session.previous_prompt_tokens
                } else {
                    prompt
                };

                app.session.input_tokens += new_tokens;
                app.session.previous_prompt_tokens = prompt;
            } else if total > completion {
                let inferred_prompt = total - completion;
                let new_tokens = if inferred_prompt > app.session.previous_prompt_tokens {
                    inferred_prompt - app.session.previous_prompt_tokens
                } else {
                    inferred_prompt
                };
                app.session.input_tokens += new_tokens;
                app.session.previous_prompt_tokens = inferred_prompt;
            } else {
                let chars: usize = app.chat_entries.iter().map(|e| e.content.len()).sum();
                app.session.input_tokens = (chars as u32) / 3;
                app.session.input_estimated = true;
            }

            app.session.output_tokens += completion;
            app.session.total_tokens = app.session.input_tokens + app.session.output_tokens;
            app.session.turn_output_tokens = completion;

            app.thinking.stop();
            app.thinking_printed = false;
            app.sync_thinking();
        }
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
        EngineEvent::Done => {
            finalize_streaming(app);

            if let Some(started) = app.turn_started_at.take() {
                let elapsed = started.elapsed();
                let tools = app.turn_tool_count;
                app.turn_status = TurnStatus::Done { elapsed, tools };
            }

            app.thinking.stop();
            app.thinking_printed = false;
            app.sync_thinking();
            app.tool_call_starts.clear();
            app.is_processing = false;
            try_process_next(app, engine, engine_event_tx);

            if app.prompt_suggestion_enabled
                && app.input.is_empty()
                && !app.suggestion_generating
                && app.last_suggestion_time.elapsed() >= std::time::Duration::from_secs(30)
            {
                app.suggestion_generating = true;

                let messages: Vec<yode_llm::types::Message> = app
                    .chat_entries
                    .iter()
                    .filter_map(|e| match e.role {
                        ChatRole::User => Some(yode_llm::types::Message::user(&e.content)),
                        ChatRole::Assistant => {
                            Some(yode_llm::types::Message::assistant(&e.content))
                        }
                        _ => None,
                    })
                    .collect();

                tracing::debug!("Generating suggestion with {} messages", messages.len());

                let engine_clone = Arc::clone(engine);
                let event_tx_clone = engine_event_tx.clone();

                tokio::spawn(async move {
                    let engine_guard = engine_clone.lock().await;
                    match engine_guard.generate_prompt_suggestion(&messages).await {
                        Some(suggestion) => {
                            tracing::debug!("Suggestion generated: {}", suggestion);
                            let _ =
                                event_tx_clone.send(EngineEvent::SuggestionReady { suggestion });
                        }
                        None => {
                            tracing::debug!("No suggestion generated");
                        }
                    }
                });
            }
        }
        EngineEvent::SuggestionReady { suggestion } => {
            app.suggestion_generating = false;
            app.last_suggestion_time = Instant::now();
            tracing::debug!("Suggestion received: {}", suggestion);
            if app.prompt_suggestion_enabled && app.input.is_empty() {
                app.prompt_suggestion = Some(suggestion);
                app.input.set_ghost_text(app.prompt_suggestion.clone());
            }
        }
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

pub(super) fn reload_provider_from_config(name: &str, app: &mut App) {
    let config = match yode_core::config::Config::load() {
        Ok(c) => c,
        Err(_) => return,
    };
    let p_config = match config.llm.providers.get(name) {
        Some(c) => c,
        None => return,
    };

    let env_prefix = name.to_uppercase().replace("-", "_");
    let api_key = std::env::var(format!("{}_API_KEY", env_prefix))
        .ok()
        .or_else(|| p_config.api_key.clone())
        .or_else(|| {
            if p_config.format == "openai" {
                std::env::var("OPENAI_API_KEY").ok()
            } else {
                std::env::var("ANTHROPIC_API_KEY")
                    .or_else(|_| std::env::var("ANTHROPIC_AUTH_TOKEN"))
                    .ok()
            }
        });

    let api_key = match api_key {
        Some(k) if !k.is_empty() => k,
        _ => return,
    };

    let default_base = if p_config.format == "openai" {
        "https://api.openai.com/v1"
    } else {
        "https://api.anthropic.com"
    };
    let base_url = std::env::var(format!("{}_BASE_URL", env_prefix))
        .ok()
        .or_else(|| p_config.base_url.clone())
        .unwrap_or_else(|| default_base.to_string());

    let provider: std::sync::Arc<dyn yode_llm::provider::LlmProvider> =
        if p_config.format == "openai" {
            std::sync::Arc::new(yode_llm::providers::openai::OpenAiProvider::new(
                name, api_key, base_url,
            ))
        } else {
            std::sync::Arc::new(yode_llm::providers::anthropic::AnthropicProvider::new(
                name, api_key, base_url,
            ))
        };

    app.provider_registry.register(provider.clone());

    if let Some(p_cfg) = config.llm.providers.get(name) {
        app.all_provider_models
            .insert(name.to_string(), p_cfg.models.clone());
    }

    if app.provider_name == name {
        app.provider_models = p_config.models.clone();
        if let Some(ref engine) = app.engine {
            if let Ok(mut eng) = engine.try_lock() {
                eng.set_provider(provider, name.to_string());
            }
        }
    }
}

pub(super) fn finalize_streaming(app: &mut App) {
    if !app.streaming_buf.is_empty()
        || !app.streaming_reasoning.is_empty()
        || !app.streaming_tag_buf.is_empty()
    {
        let mut content_raw = std::mem::take(&mut app.streaming_buf);
        content_raw.push_str(&std::mem::take(&mut app.streaming_tag_buf));

        let content = strip_internal_tags(&content_raw);
        let reasoning = if app.streaming_reasoning.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut app.streaming_reasoning))
        };
        let all_lines: Vec<&str> = content.lines().collect();
        let printed = app.streaming_printed_lines;

        if printed < all_lines.len() {
            let remainder: Vec<String> =
                all_lines[printed..].iter().map(|s| s.to_string()).collect();
            app.streaming_remainder = Some((remainder, printed == 0));
        }

        let mut entry =
            ChatEntry::new_with_reasoning(ChatRole::Assistant, content.clone(), reasoning);
        entry.already_printed = true;
        if !content.trim().is_empty() || entry.reasoning.is_some() {
            app.chat_entries.push(entry);
        }
        app.streaming_printed_lines = 0;
        app.streaming_in_code_block = false;
    }
}
