use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, Mutex};

use yode_core::engine::{AgentEngine, EngineEvent};
use yode_llm::types::ChatResponse;

use crate::runtime_artifacts::{
    write_prompt_cache_artifact, write_prompt_cache_break_artifact,
    write_prompt_cache_event_artifact, write_prompt_cache_state_artifact,
};
use crate::runtime_display::format_turn_completed_message;
use crate::ui::chat::{
    render_markdown_ansi_white_with_options, streaming_markdown_advance_stable_boundary,
};

use super::super::turn_flow::try_process_next;
use super::super::{
    find_case_insensitive, strip_internal_tags, App, ChatEntry, ChatRole, TurnStatus, TAG_RE,
};

pub(super) fn handle_text_delta(app: &mut App, delta: String) {
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

pub(super) fn handle_reasoning_delta(app: &mut App, delta: String) {
    app.received_reasoning_delta = true;
    app.streaming_reasoning.push_str(&delta);
}

pub(super) fn handle_text_complete(app: &mut App, text: String) {
    if app.streaming_buf.is_empty() {
        app.streaming_buf = text;
    }
}

pub(super) fn handle_reasoning_complete(app: &mut App, text: String) {
    if app.streaming_reasoning.is_empty() {
        app.streaming_reasoning = text;
    }
}

pub(super) fn handle_turn_complete(app: &mut App, response: ChatResponse) {
    finalize_streaming(app);

    let prompt = response.usage.prompt_tokens;
    let completion = response.usage.completion_tokens;
    let total = response.usage.total_tokens;
    let mut turn_input_tokens = app.session.turn_input_tokens;

    if prompt > 0 {
        let new_tokens = if prompt > app.session.previous_prompt_tokens {
            prompt - app.session.previous_prompt_tokens
        } else {
            prompt
        };

        app.session.input_tokens += new_tokens;
        app.session.previous_prompt_tokens = prompt;
        if new_tokens > 0 {
            turn_input_tokens = new_tokens;
        }
    } else if total > completion {
        let inferred_prompt = total - completion;
        let new_tokens = if inferred_prompt > app.session.previous_prompt_tokens {
            inferred_prompt - app.session.previous_prompt_tokens
        } else {
            inferred_prompt
        };
        app.session.input_tokens += new_tokens;
        app.session.previous_prompt_tokens = inferred_prompt;
        if new_tokens > 0 {
            turn_input_tokens = new_tokens;
        }
    } else {
        let chars: usize = app.chat_entries.iter().map(|e| e.content.len()).sum();
        turn_input_tokens = (chars as u32) / 3;
        app.session.input_tokens = turn_input_tokens;
        app.session.input_estimated = true;
    }

    app.session.output_tokens += completion;
    app.session.total_tokens = app.session.input_tokens + app.session.output_tokens;
    app.session.turn_input_tokens = turn_input_tokens;
    app.session.turn_output_tokens = completion;

    app.thinking.stop();
    app.thinking_printed = false;
    app.sync_thinking();
}

pub(super) fn handle_done(
    app: &mut App,
    engine: &Arc<Mutex<AgentEngine>>,
    engine_event_tx: &mpsc::UnboundedSender<EngineEvent>,
) {
    finalize_streaming(app);
    let runtime_state = engine.try_lock().ok().map(|engine| engine.runtime_state());
    if let Some(state) = runtime_state.as_ref() {
        let project_root = std::path::PathBuf::from(&app.session.working_dir);
        let _ = write_prompt_cache_artifact(&project_root, &app.session.session_id, state);
        let _ = write_prompt_cache_state_artifact(&project_root, &app.session.session_id, state);
        let _ = write_prompt_cache_event_artifact(&project_root, &app.session.session_id, state);
        let _ = write_prompt_cache_break_artifact(&project_root, &app.session.session_id, state);
    }

    if let Some(started) = app.turn_started_at.take() {
        let elapsed = started.elapsed();
        let tools = app.turn_tool_count;
        app.last_turn_completion_message = Some(format_turn_completed_message(
            elapsed,
            tools,
            app.session.turn_input_tokens,
            app.session.turn_output_tokens,
            app.session.total_tokens,
            app.session.tool_call_count,
            runtime_state.as_ref(),
        ));
        app.turn_status = TurnStatus::Done { elapsed, tools };
        app.turn_done_at = Some(Instant::now());
    } else {
        app.turn_status = TurnStatus::Idle;
        app.turn_done_at = None;
    }
    app.thinking.stop();
    app.thinking_printed = false;
    app.sync_thinking();
    app.tool_call_starts.clear();
    app.is_processing = false;
    try_process_next(app, engine, engine_event_tx);

    if app.prompt_suggestion.enabled
        && app.input.is_empty()
        && !app.prompt_suggestion.generating
        && app.prompt_suggestion.last_generated_at.elapsed() >= Duration::from_secs(30)
    {
        app.prompt_suggestion.generating = true;

        let messages: Vec<yode_llm::types::Message> = app
            .chat_entries
            .iter()
            .filter_map(|e| match e.role {
                ChatRole::User => Some(yode_llm::types::Message::user(&e.content)),
                ChatRole::Assistant => Some(yode_llm::types::Message::assistant(&e.content)),
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
                    let _ = event_tx_clone.send(EngineEvent::SuggestionReady { suggestion });
                }
                None => {
                    tracing::debug!("No suggestion generated");
                }
            }
        });
    }
}

pub(super) fn handle_suggestion_ready(app: &mut App, suggestion: String) {
    app.prompt_suggestion.generating = false;
    app.prompt_suggestion.last_generated_at = Instant::now();
    tracing::debug!("Suggestion received: {}", suggestion);
    if app.prompt_suggestion.enabled && app.input.is_empty() {
        app.prompt_suggestion.value = Some(suggestion);
        app.input
            .set_ghost_text(app.prompt_suggestion.value.clone());
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
        let stable_end =
            streaming_markdown_advance_stable_boundary(&content, app.streaming_markdown_stable_len);
        let stable_len = app
            .streaming_markdown_stable_len
            .min(stable_end)
            .min(content.len());
        let remainder = &content[stable_len..];
        app.streaming_markdown_remainder = None;
        if !remainder.trim().is_empty() {
            let render_width = crossterm::terminal::size()
                .map(|(width, _)| width.saturating_sub(2) as usize)
                .unwrap_or(78);
            let rendered = render_markdown_ansi_white_with_options(
                remainder,
                Some(render_width),
                app.terminal_caps.supports_hyperlinks(),
            );
            if !rendered.is_empty() {
                app.streaming_markdown_remainder = Some((rendered, stable_len == 0));
            }
        }

        let mut entry =
            ChatEntry::new_with_reasoning(ChatRole::Assistant, content.clone(), reasoning);
        entry.already_printed = true;
        if !content.trim().is_empty() || entry.reasoning.is_some() {
            app.chat_entries.push(entry);
        }
        app.streaming_markdown_stable_len = 0;
        app.streaming_markdown_cached_buf_len = 0;
        app.streaming_markdown_cached_width = 0;
        app.streaming_markdown_preview_source.clear();
        app.streaming_markdown_preview.clear();
    }
}
