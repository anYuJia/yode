use super::*;

impl AgentEngine {
    /// Get the current message history.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Get the context.
    pub fn context(&self) -> &AgentContext {
        &self.context
    }

    /// Restore messages from database for a resumed session.
    pub fn restore_messages(&mut self, messages: Vec<Message>) {
        let system_msg = self.messages.first().cloned();
        self.messages.clear();
        self.failed_tool_call_ids.clear();
        if let Some(sys) = system_msg {
            self.messages.push(sys);
        }
        self.messages.extend(messages);
        self.reset_autocompact_state();
        self.compaction_cause_histogram.clear();
        self.rebuild_runtime_artifact_state_from_disk();
        info!(
            "Restored {} messages from database",
            self.messages.len() - 1
        );
    }

    /// Clear conversation history, keeping only the system prompt.
    pub fn clear_conversation(&mut self) {
        if self.messages.len() > 1 {
            let system_msg = self.messages.first().cloned();
            self.messages.clear();
            self.failed_tool_call_ids.clear();
            if let Some(sys) = system_msg {
                self.messages.push(sys);
            }
            info!("Cleared conversation, kept system prompt");
        }
        if let Err(err) = clear_live_session_memory(&self.context.working_dir_compat()) {
            warn!(
                "Failed to clear live session memory during conversation reset: {}",
                err
            );
        }
        self.reset_live_session_memory_tracking();
        self.last_compaction_mode = None;
        self.last_compaction_at = None;
        self.last_compaction_summary_excerpt = None;
        self.last_compaction_session_memory_path = None;
        self.last_compaction_transcript_path = None;
        self.total_compactions = 0;
        self.auto_compactions = 0;
        self.manual_compactions = 0;
        self.compaction_cause_histogram.clear();
        self.set_shared_memory_status(None, None, false, 0);
        self.sync_persisted_messages_snapshot();
        self.rebuild_system_prompt();
        self.reset_autocompact_state();
    }

    /// Save a message to the database if available.
    pub(super) fn persist_message(
        &self,
        role: &str,
        content: Option<&str>,
        reasoning: Option<&str>,
        tool_calls_json: Option<&str>,
        tool_call_id: Option<&str>,
    ) {
        if let Some(ref db) = self.db {
            if let Err(e) = db.save_message(
                &self.context.session_id,
                role,
                content,
                reasoning,
                tool_calls_json,
                tool_call_id,
            ) {
                warn!("Failed to persist message: {}", e);
            }
            if let Err(e) = db.touch_session(&self.context.session_id) {
                warn!("Failed to touch session: {}", e);
            }
        }
    }

    pub(super) fn persist_session_artifacts(&self) {
        let Some(ref db) = self.db else {
            return;
        };

        let shared = self.shared_memory_status.try_lock().ok().map(|state| {
            (
                state.last_session_memory_update_at.clone(),
                state.last_session_memory_update_path.clone(),
                state.last_session_memory_generated_summary,
            )
        });

        let artifacts = SessionArtifacts {
            last_compaction_mode: self.last_compaction_mode.clone(),
            last_compaction_at: self.last_compaction_at.clone(),
            last_compaction_summary_excerpt: self.last_compaction_summary_excerpt.clone(),
            last_compaction_session_memory_path: self.last_compaction_session_memory_path.clone(),
            last_compaction_transcript_path: self.last_compaction_transcript_path.clone(),
            last_session_memory_update_at: shared.as_ref().and_then(|s| s.0.clone()),
            last_session_memory_update_path: shared.as_ref().and_then(|s| s.1.clone()),
            last_session_memory_generated_summary: shared.map(|s| s.2).unwrap_or(false),
        };

        if let Err(err) = db.upsert_session_artifacts(&self.context.session_id, &artifacts) {
            warn!("Failed to persist session artifacts: {}", err);
        }
    }

    pub(super) fn rebuild_runtime_artifact_state_from_disk(&mut self) {
        let project_root = self.context.working_dir_compat();
        self.last_compaction_mode = None;
        self.last_compaction_at = None;
        self.last_compaction_summary_excerpt = None;
        self.last_compaction_session_memory_path = None;
        self.last_compaction_transcript_path = None;

        if let Some((path, state)) = latest_transcript_runtime_state(&project_root) {
            self.last_compaction_mode = state.mode;
            self.last_compaction_at = state.timestamp;
            self.last_compaction_summary_excerpt = state.summary_excerpt;
            self.last_compaction_session_memory_path = state.session_memory_path.or_else(|| {
                let session_path = crate::session_memory::session_memory_path(&project_root);
                session_path
                    .exists()
                    .then(|| session_path.display().to_string())
            });
            self.last_compaction_transcript_path = Some(path.display().to_string());
        } else {
            let session_path = crate::session_memory::session_memory_path(&project_root);
            if session_path.exists() {
                self.last_compaction_session_memory_path = Some(session_path.display().to_string());
            }
        }

        let live_path = crate::session_memory::live_session_memory_path(&project_root);
        if let Ok(meta) = std::fs::metadata(&live_path) {
            let updated_at = meta.modified().ok().map(|modified| {
                chrono::DateTime::<chrono::Local>::from(modified)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            });
            if let Ok(mut state) = self.shared_memory_status.try_lock() {
                state.last_session_memory_update_at = updated_at;
                state.last_session_memory_update_path = Some(live_path.display().to_string());
                state.last_session_memory_generated_summary = false;
            }
        }
    }

    pub(super) fn record_tool_result_status(&mut self, tool_call_id: &str, result: &ToolResult) {
        if result.is_error {
            self.failed_tool_call_ids.insert(tool_call_id.to_string());
        } else {
            self.failed_tool_call_ids.remove(tool_call_id);
        }
    }

    pub(super) fn sync_persisted_messages_snapshot(&self) {
        let Some(ref db) = self.db else {
            return;
        };

        let snapshot = self.messages.iter().skip(1).cloned().collect::<Vec<_>>();

        if let Err(err) = db.replace_messages(&self.context.session_id, &snapshot) {
            warn!("Failed to rewrite session message snapshot: {}", err);
            return;
        }

        if let Err(err) = db.touch_session(&self.context.session_id) {
            warn!("Failed to touch session after snapshot rewrite: {}", err);
        }
    }

    pub(super) fn set_shared_memory_status(
        &self,
        updated_at: Option<String>,
        path: Option<String>,
        generated_summary: bool,
        count_delta: u32,
    ) {
        if let Ok(mut state) = self.shared_memory_status.try_lock() {
            state.last_session_memory_update_at = updated_at;
            state.last_session_memory_update_path = path;
            state.last_session_memory_generated_summary = generated_summary;
            if state.last_session_memory_update_at.is_none()
                && state.last_session_memory_update_path.is_none()
                && count_delta == 0
            {
                state.session_memory_update_count = 0;
            } else {
                state.session_memory_update_count = state
                    .session_memory_update_count
                    .saturating_add(count_delta);
            }
        }
        self.persist_session_artifacts();
    }

    pub(super) fn current_message_char_count(&self) -> usize {
        self.messages
            .iter()
            .map(|m| {
                m.content.as_ref().map(|c| c.len()).unwrap_or(0)
                    + m.tool_calls
                        .iter()
                        .map(|tc| tc.arguments.len() + tc.name.len())
                        .sum::<usize>()
            })
            .sum()
    }

    pub(super) fn maybe_refresh_live_session_memory(
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
                .and_then(|resp| resp.message.content)
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

    pub(super) fn flush_live_session_memory_on_shutdown(&mut self) {
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

    pub(super) fn invalidate_live_session_memory_updates(&mut self) {
        self.session_memory_generation
            .fetch_add(1, Ordering::SeqCst);
        self.session_memory_update_in_progress
            .store(false, Ordering::SeqCst);
    }

    pub(super) fn reset_live_session_memory_tracking(&mut self) {
        self.session_tool_calls_total = 0;
        self.session_memory_initialized = false;
        self.last_session_memory_char_count = 0;
        self.last_session_memory_tool_count = 0;
        self.invalidate_live_session_memory_updates();
    }

    pub(super) fn reset_autocompact_state(&mut self) {
        self.compaction_failures = 0;
        self.autocompact_disabled = false;
        self.compaction_in_progress = false;
        self.last_compaction_breaker_reason = None;
    }

    pub(super) fn record_compaction_failure(
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

        let already_present = self.messages.iter().any(|msg| {
            matches!(msg.role, Role::System) && msg.content.as_deref() == Some(warning.as_str())
        });
        if !already_present {
            self.messages.push(Message::system(warning.clone()));
        }

        let _ = event_tx.send(EngineEvent::Error(warning));
    }
}
