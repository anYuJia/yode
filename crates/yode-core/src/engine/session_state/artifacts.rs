use super::*;

impl AgentEngine {
    /// Save a message to the database if available.
    pub(in crate::engine) fn persist_message(
        &self,
        role: &str,
        content: Option<&str>,
        reasoning: Option<&str>,
        tool_calls_json: Option<&str>,
        tool_call_id: Option<&str>,
    ) {
        if let Some(db) = &self.db {
            if let Err(err) = db.save_message(
                &self.context.session_id,
                role,
                content,
                reasoning,
                tool_calls_json,
                tool_call_id,
            ) {
                warn!("Failed to persist message: {}", err);
            }
            if let Err(err) = db.touch_session(&self.context.session_id) {
                warn!("Failed to touch session: {}", err);
            }
        }
    }

    pub(in crate::engine) fn persist_session_artifacts(&self) {
        let Some(db) = &self.db else {
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
            last_session_memory_update_at: shared.as_ref().and_then(|shared| shared.0.clone()),
            last_session_memory_update_path: shared.as_ref().and_then(|shared| shared.1.clone()),
            last_session_memory_generated_summary: shared.map(|shared| shared.2).unwrap_or(false),
        };

        if let Err(err) = db.upsert_session_artifacts(&self.context.session_id, &artifacts) {
            warn!("Failed to persist session artifacts: {}", err);
        }
    }

    pub(in crate::engine) fn rebuild_runtime_artifact_state_from_disk(&mut self) {
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
        if let Ok(metadata) = std::fs::metadata(&live_path) {
            let updated_at = metadata.modified().ok().map(|modified| {
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

    pub(in crate::engine) fn record_tool_result_status(
        &mut self,
        tool_call_id: &str,
        result: &ToolResult,
    ) {
        if result.is_error {
            self.failed_tool_call_ids.insert(tool_call_id.to_string());
        } else {
            self.failed_tool_call_ids.remove(tool_call_id);
        }
    }

    pub(in crate::engine) fn sync_persisted_messages_snapshot(&self) {
        let Some(db) = &self.db else {
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

    pub(in crate::engine) fn set_shared_memory_status(
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

    pub(in crate::engine) fn current_message_char_count(&self) -> usize {
        self.messages
            .iter()
            .map(|message| {
                message
                    .content
                    .as_ref()
                    .map(|content| content.len())
                    .unwrap_or(0)
                    + message
                        .tool_calls
                        .iter()
                        .map(|tool_call| tool_call.arguments.len() + tool_call.name.len())
                        .sum::<usize>()
            })
            .sum()
    }
}
