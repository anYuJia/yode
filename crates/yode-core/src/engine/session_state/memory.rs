use super::*;

impl AgentEngine {
    pub(in crate::engine) fn maybe_refresh_live_session_memory(
        &mut self,
        event_tx: Option<&mpsc::UnboundedSender<EngineEvent>>,
    ) {
        self.session_tool_calls_total = self
            .session_tool_calls_total
            .saturating_add(self.tool_call_count);

        let current_chars = self.current_message_char_count();
        if !self.session_memory_initialized {
            if current_chars < SESSION_MEMORY_INIT_CHAR_THRESHOLD
                && self.session_tool_calls_total < SESSION_MEMORY_TOOL_DELTA_THRESHOLD
            {
                return;
            }
            self.session_memory_initialized = true;
        }

        let char_delta = current_chars.saturating_sub(self.last_session_memory_char_count);
        let tool_delta = self
            .session_tool_calls_total
            .saturating_sub(self.last_session_memory_tool_count);

        if char_delta < SESSION_MEMORY_CHAR_DELTA_THRESHOLD
            && tool_delta < SESSION_MEMORY_TOOL_DELTA_THRESHOLD
        {
            return;
        }

        self.last_session_memory_char_count = current_chars;
        self.last_session_memory_tool_count = self.session_tool_calls_total;

        let snapshot = build_live_snapshot(
            &self.context.session_id,
            &self.messages,
            self.session_tool_calls_total,
            &self.files_read.keys().cloned().collect::<Vec<_>>(),
            &self.files_modified,
        );

        let project_root = self.context.working_dir_compat();
        if self.provider.name() == "mock" {
            match persist_live_session_memory(&project_root, &snapshot) {
                Ok(path) => {
                    let updated_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                    let path_str = path.display().to_string();
                    self.set_shared_memory_status(
                        Some(updated_at),
                        Some(path_str.clone()),
                        false,
                        1,
                    );
                    self.rebuild_system_prompt();
                    if let Some(event_tx) = event_tx {
                        let _ = event_tx.send(EngineEvent::SessionMemoryUpdated {
                            path: path_str,
                            generated_summary: false,
                        });
                    }
                }
                Err(err) => warn!("Failed to refresh live session memory: {}", err),
            }
            return;
        }

        if self
            .session_memory_update_in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        let provider = Arc::clone(&self.provider);
        let model = self.context.model.clone();
        let generation = Arc::clone(&self.session_memory_generation);
        let scheduled_generation = generation.load(Ordering::SeqCst);
        let update_flag = Arc::clone(&self.session_memory_update_in_progress);
        let shared_memory_status = Arc::clone(&self.shared_memory_status);
        let event_tx = event_tx.cloned();
        let recent_messages = self
            .messages
            .iter()
            .rev()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();

        tokio::spawn(async move {
            let existing_summary =
                std::fs::read_to_string(live_session_memory_path(&project_root)).ok();
            let prompt = render_live_session_memory_prompt(
                existing_summary.as_deref(),
                &snapshot,
                &recent_messages,
            );
            let request = ChatRequest {
                model,
                messages: vec![
                    Message::system(
                        "You maintain concise session memory for a coding assistant. Return markdown only.",
                    ),
                    Message::user(prompt),
                ],
                tools: vec![],
                temperature: Some(0.2),
                max_tokens: Some(500),
            };

            let summary = provider
                .chat(request)
                .await
                .ok()
                .and_then(|response| response.message.content)
                .filter(|content| !content.trim().is_empty());

            if generation.load(Ordering::SeqCst) != scheduled_generation {
                update_flag.store(false, Ordering::SeqCst);
                return;
            }

            let result = if let Some(summary) = summary {
                persist_live_session_memory_summary(&project_root, &snapshot, &summary)
                    .map(|path| (path, true))
            } else {
                persist_live_session_memory(&project_root, &snapshot).map(|path| (path, false))
            };

            match result {
                Ok((path, generated_summary)) => {
                    info!("Live session memory refreshed asynchronously");
                    let updated_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                    let path_str = path.display().to_string();
                    let mut state = shared_memory_status.lock().await;
                    state.last_session_memory_update_at = Some(updated_at.clone());
                    state.last_session_memory_update_path = Some(path_str.clone());
                    state.last_session_memory_generated_summary = generated_summary;
                    state.session_memory_update_count =
                        state.session_memory_update_count.saturating_add(1);
                    drop(state);
                    if let Some(event_tx) = &event_tx {
                        let _ = event_tx.send(EngineEvent::SessionMemoryUpdated {
                            path: path_str,
                            generated_summary,
                        });
                    }
                }
                Err(err) => {
                    warn!("Failed to persist async live session memory: {}", err);
                }
            }

            update_flag.store(false, Ordering::SeqCst);
        });
    }

    pub(in crate::engine) fn flush_live_session_memory_on_shutdown(&mut self) {
        self.invalidate_live_session_memory_updates();

        if self.messages.len() <= 1 {
            return;
        }

        let snapshot = build_live_snapshot(
            &self.context.session_id,
            &self.messages,
            self.session_tool_calls_total,
            &self.files_read.keys().cloned().collect::<Vec<_>>(),
            &self.files_modified,
        );

        match persist_live_session_memory(&self.context.working_dir_compat(), &snapshot) {
            Ok(path) => {
                self.set_shared_memory_status(
                    Some(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()),
                    Some(path.display().to_string()),
                    false,
                    1,
                );
            }
            Err(err) => {
                warn!("Failed to flush live session memory on shutdown: {}", err);
            }
        }
    }

    pub(in crate::engine) fn invalidate_live_session_memory_updates(&mut self) {
        self.session_memory_generation
            .fetch_add(1, Ordering::SeqCst);
        self.session_memory_update_in_progress
            .store(false, Ordering::SeqCst);
    }

    pub(in crate::engine) fn reset_live_session_memory_tracking(&mut self) {
        self.session_tool_calls_total = 0;
        self.session_memory_initialized = false;
        self.last_session_memory_char_count = 0;
        self.last_session_memory_tool_count = 0;
        self.invalidate_live_session_memory_updates();
    }

    pub(in crate::engine) fn reset_autocompact_state(&mut self) {
        self.compaction_failures = 0;
        self.autocompact_disabled = false;
        self.compaction_in_progress = false;
        self.last_compaction_breaker_reason = None;
    }

    pub(in crate::engine) fn record_compaction_failure(
        &mut self,
        reason: impl Into<String>,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) {
        let reason = reason.into();
        self.compaction_failures += 1;
        warn!(
            "Auto-compaction failure {}/{}: {}",
            self.compaction_failures, MAX_CONSECUTIVE_COMPACTION_FAILURES, reason
        );

        if self.compaction_failures < MAX_CONSECUTIVE_COMPACTION_FAILURES {
            return;
        }

        self.autocompact_disabled = true;
        self.last_compaction_breaker_reason = Some(reason.clone());
        let warning = format!(
            "[Auto-compact disabled after {} consecutive failures: {}. Continue with shorter turns or clear context before retrying.]",
            self.compaction_failures, reason
        );

        let already_present = self.messages.iter().any(|message| {
            matches!(message.role, Role::System)
                && message.content.as_deref() == Some(warning.as_str())
        });
        if !already_present {
            self.messages.push(Message::system(warning.clone()));
        }

        let _ = event_tx.send(EngineEvent::Error(warning));
    }
}
