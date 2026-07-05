use super::*;

#[derive(Debug, Default, serde::Deserialize)]
struct PromptCacheArtifactState {
    #[serde(default)]
    last_turn_prompt_tokens: Option<u32>,
    #[serde(default)]
    last_turn_completion_tokens: Option<u32>,
    #[serde(default)]
    last_turn_cache_write_tokens: Option<u32>,
    #[serde(default)]
    last_turn_cache_read_tokens: Option<u32>,
    #[serde(default)]
    last_turn_cache_edit_deletions: Option<u32>,
    #[serde(default)]
    last_turn_cache_deleted_tokens: Option<u32>,
    #[serde(default)]
    pending_cache_edit_refs: u32,
    #[serde(default)]
    pinned_cache_edit_refs: u32,
    #[serde(default)]
    pending_cache_edit_ref_values: Vec<String>,
    #[serde(default)]
    pinned_cache_edit_ref_values: Vec<String>,
    #[serde(default)]
    prompt_cache_break_count: u32,
    #[serde(default)]
    last_prompt_cache_break_reason: Option<String>,
    #[serde(default)]
    last_prompt_cache_break_at: Option<String>,
    #[serde(default)]
    last_prompt_cache_expected_drop_reason: Option<String>,
    #[serde(default)]
    last_prompt_cache_change_summary: Option<String>,
    #[serde(default)]
    last_prompt_cache_transition_kind: Option<String>,
    #[serde(default)]
    last_prompt_cache_transition_reason: Option<String>,
    #[serde(default)]
    last_prompt_cache_prefix_hash: Option<String>,
    #[serde(default)]
    last_prompt_cache_system_hash: Option<String>,
    #[serde(default)]
    last_prompt_cache_restore_hash: Option<String>,
    #[serde(default)]
    last_prompt_cache_tool_hash: Option<String>,
    #[serde(default)]
    last_prompt_cache_message_hash: Option<String>,
    #[serde(default)]
    last_prompt_cache_diff_artifact_path: Option<String>,
    #[serde(default)]
    last_prompt_cache_diff_summary: Option<String>,
    #[serde(default)]
    reported_turns: u32,
    #[serde(default)]
    cache_write_turns: u32,
    #[serde(default)]
    cache_read_turns: u32,
    #[serde(default)]
    cache_edit_turns: u32,
    #[serde(default)]
    cache_write_tokens_total: u64,
    #[serde(default)]
    cache_read_tokens_total: u64,
    #[serde(default)]
    cache_edit_deletions_total: u64,
    #[serde(default)]
    cache_deleted_tokens_total: u64,
}

fn prompt_cache_state_artifact_path(
    project_root: &std::path::Path,
    session_id: &str,
) -> std::path::PathBuf {
    let short_session = session_id.chars().take(8).collect::<String>();
    project_root
        .join(".yode")
        .join("status")
        .join(format!("{}-prompt-cache-state.json", short_session))
}

fn prompt_cache_diff_artifact_path(
    project_root: &std::path::Path,
    session_id: &str,
) -> std::path::PathBuf {
    let short_session = session_id.chars().take(8).collect::<String>();
    project_root
        .join(".yode")
        .join("status")
        .join(format!("{}-prompt-cache-diff.md", short_session))
}

fn load_prompt_cache_state_artifact(
    project_root: &std::path::Path,
    session_id: &str,
) -> Option<PromptCacheArtifactState> {
    let path = prompt_cache_state_artifact_path(project_root, session_id);
    let content = std::fs::read_to_string(path).ok()?;
    parse_prompt_cache_state_artifact_content(&content)
}

async fn load_prompt_cache_state_artifact_async(
    project_root: &std::path::Path,
    session_id: &str,
) -> Option<PromptCacheArtifactState> {
    let path = prompt_cache_state_artifact_path(project_root, session_id);
    let content = tokio::fs::read_to_string(path).await.ok()?;
    parse_prompt_cache_state_artifact_content(&content)
}

fn parse_prompt_cache_state_artifact_content(content: &str) -> Option<PromptCacheArtifactState> {
    serde_json::from_str::<PromptCacheArtifactState>(content).ok()
}

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
        self.persist_message_with_images(
            role,
            content,
            reasoning,
            tool_calls_json,
            tool_call_id,
            None,
        );
    }

    pub(in crate::engine) fn persist_message_with_images(
        &self,
        role: &str,
        content: Option<&str>,
        reasoning: Option<&str>,
        tool_calls_json: Option<&str>,
        tool_call_id: Option<&str>,
        images: Option<&[yode_llm::types::ImageData]>,
    ) {
        if let Some(db) = &self.db {
            if let Err(err) = db.save_message_with_images(
                &self.context.session_id,
                role,
                content,
                reasoning,
                tool_calls_json,
                tool_call_id,
                images,
            ) {
                warn!("Failed to persist message: {}", err);
            }
            if let Err(err) = db.touch_session(&self.context.session_id) {
                warn!("Failed to touch session: {}", err);
            }
        }
    }

    pub(in crate::engine) fn persist_message_with_metadata(
        &self,
        role: &str,
        content: Option<&str>,
        reasoning: Option<&str>,
        tool_calls_json: Option<&str>,
        tool_call_id: Option<&str>,
        metadata: Option<&serde_json::Value>,
    ) {
        if let Some(db) = &self.db {
            if let Err(err) = db.save_message_with_metadata(
                &self.context.session_id,
                role,
                content,
                reasoning,
                tool_calls_json,
                tool_call_id,
                metadata,
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
            last_compact_boundary_json: self
                .last_compact_boundary
                .as_ref()
                .and_then(|boundary| serde_json::to_string(boundary).ok()),
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
        self.last_compact_boundary = None;

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
            self.last_compact_boundary = state.compact_boundary;
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

        if let Some(cache) =
            load_prompt_cache_state_artifact(&project_root, &self.context.session_id)
        {
            self.pending_cache_edit_refs = cache.pending_cache_edit_ref_values;
            self.pending_cache_edit_refs.sort();
            self.pending_cache_edit_refs.dedup();

            self.pinned_cache_edit_refs = cache.pinned_cache_edit_ref_values;
            self.pinned_cache_edit_refs.sort();
            self.pinned_cache_edit_refs.dedup();
            self.pinned_cache_edit_refs
                .retain(|cache_ref| !self.pending_cache_edit_refs.contains(cache_ref));

            self.prompt_cache_runtime.last_turn_prompt_tokens = cache.last_turn_prompt_tokens;
            self.prompt_cache_runtime.last_turn_completion_tokens =
                cache.last_turn_completion_tokens;
            self.prompt_cache_runtime.last_turn_cache_write_tokens =
                cache.last_turn_cache_write_tokens;
            self.prompt_cache_runtime.last_turn_cache_read_tokens =
                cache.last_turn_cache_read_tokens;
            self.prompt_cache_runtime.last_turn_cache_edit_deletions =
                cache.last_turn_cache_edit_deletions;
            self.prompt_cache_runtime.last_turn_cache_deleted_tokens =
                cache.last_turn_cache_deleted_tokens;
            self.prompt_cache_runtime.pending_cache_edit_refs =
                if self.pending_cache_edit_refs.is_empty() {
                    cache.pending_cache_edit_refs
                } else {
                    self.pending_cache_edit_refs.len() as u32
                };
            self.prompt_cache_runtime.pinned_cache_edit_refs =
                if self.pinned_cache_edit_refs.is_empty() {
                    cache.pinned_cache_edit_refs
                } else {
                    self.pinned_cache_edit_refs.len() as u32
                };
            self.prompt_cache_runtime.prompt_cache_break_count = cache.prompt_cache_break_count;
            self.prompt_cache_runtime.last_prompt_cache_break_reason =
                cache.last_prompt_cache_break_reason;
            self.prompt_cache_runtime.last_prompt_cache_break_at = cache.last_prompt_cache_break_at;
            self.prompt_cache_runtime
                .last_prompt_cache_expected_drop_reason =
                cache.last_prompt_cache_expected_drop_reason;
            self.prompt_cache_runtime.last_prompt_cache_change_summary =
                cache.last_prompt_cache_change_summary;
            self.prompt_cache_runtime.last_prompt_cache_transition_kind =
                cache.last_prompt_cache_transition_kind;
            self.prompt_cache_runtime
                .last_prompt_cache_transition_reason = cache.last_prompt_cache_transition_reason;
            self.prompt_cache_runtime
                .last_prompt_cache_diff_artifact_path =
                cache.last_prompt_cache_diff_artifact_path.or_else(|| {
                    let path =
                        prompt_cache_diff_artifact_path(&project_root, &self.context.session_id);
                    path.exists().then(|| path.display().to_string())
                });
            self.prompt_cache_runtime.last_prompt_cache_diff_summary =
                cache.last_prompt_cache_diff_summary;
            self.prompt_cache_runtime.reported_turns = cache.reported_turns;
            self.prompt_cache_runtime.cache_write_turns = cache.cache_write_turns;
            self.prompt_cache_runtime.cache_read_turns = cache.cache_read_turns;
            self.prompt_cache_runtime.cache_edit_turns = cache.cache_edit_turns;
            self.prompt_cache_runtime.cache_write_tokens_total = cache.cache_write_tokens_total;
            self.prompt_cache_runtime.cache_read_tokens_total = cache.cache_read_tokens_total;
            self.prompt_cache_runtime.cache_edit_deletions_total = cache.cache_edit_deletions_total;
            self.prompt_cache_runtime.cache_deleted_tokens_total = cache.cache_deleted_tokens_total;

            self.last_prompt_cache_prefix_hash = cache.last_prompt_cache_prefix_hash;
            self.last_prompt_cache_system_hash = cache.last_prompt_cache_system_hash;
            self.last_prompt_cache_restore_hash = cache.last_prompt_cache_restore_hash;
            self.last_prompt_cache_tool_hash = cache.last_prompt_cache_tool_hash;
            self.last_prompt_cache_message_hash = cache.last_prompt_cache_message_hash;
        }

        if let Some(reason) = self.forced_prompt_cache_expected_drop_reason.clone() {
            self.prompt_cache_runtime
                .last_prompt_cache_expected_drop_reason = Some(reason);
        }
    }

    pub(in crate::engine) async fn rebuild_runtime_artifact_state_from_disk_async(&mut self) {
        let project_root = self.context.working_dir_compat();
        self.last_compaction_mode = None;
        self.last_compaction_at = None;
        self.last_compaction_summary_excerpt = None;
        self.last_compaction_session_memory_path = None;
        self.last_compaction_transcript_path = None;
        self.last_compact_boundary = None;

        if let Some((path, state)) = latest_transcript_runtime_state_async(&project_root).await {
            self.last_compaction_mode = state.mode;
            self.last_compaction_at = state.timestamp;
            self.last_compaction_summary_excerpt = state.summary_excerpt;
            self.last_compaction_session_memory_path = match state.session_memory_path {
                Some(path) => Some(path),
                None => {
                    let session_path = crate::session_memory::session_memory_path(&project_root);
                    tokio::fs::try_exists(&session_path)
                        .await
                        .unwrap_or(false)
                        .then(|| session_path.display().to_string())
                }
            };
            self.last_compaction_transcript_path = Some(path.display().to_string());
            self.last_compact_boundary = state.compact_boundary;
        } else {
            let session_path = crate::session_memory::session_memory_path(&project_root);
            if tokio::fs::try_exists(&session_path).await.unwrap_or(false) {
                self.last_compaction_session_memory_path = Some(session_path.display().to_string());
            }
        }

        let live_path = crate::session_memory::live_session_memory_path(&project_root);
        if let Ok(metadata) = tokio::fs::metadata(&live_path).await {
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

        if let Some(cache) =
            load_prompt_cache_state_artifact_async(&project_root, &self.context.session_id).await
        {
            self.pending_cache_edit_refs = cache.pending_cache_edit_ref_values;
            self.pending_cache_edit_refs.sort();
            self.pending_cache_edit_refs.dedup();

            self.pinned_cache_edit_refs = cache.pinned_cache_edit_ref_values;
            self.pinned_cache_edit_refs.sort();
            self.pinned_cache_edit_refs.dedup();
            self.pinned_cache_edit_refs
                .retain(|cache_ref| !self.pending_cache_edit_refs.contains(cache_ref));

            self.prompt_cache_runtime.last_turn_prompt_tokens = cache.last_turn_prompt_tokens;
            self.prompt_cache_runtime.last_turn_completion_tokens =
                cache.last_turn_completion_tokens;
            self.prompt_cache_runtime.last_turn_cache_write_tokens =
                cache.last_turn_cache_write_tokens;
            self.prompt_cache_runtime.last_turn_cache_read_tokens =
                cache.last_turn_cache_read_tokens;
            self.prompt_cache_runtime.last_turn_cache_edit_deletions =
                cache.last_turn_cache_edit_deletions;
            self.prompt_cache_runtime.last_turn_cache_deleted_tokens =
                cache.last_turn_cache_deleted_tokens;
            self.prompt_cache_runtime.pending_cache_edit_refs =
                if self.pending_cache_edit_refs.is_empty() {
                    cache.pending_cache_edit_refs
                } else {
                    self.pending_cache_edit_refs.len() as u32
                };
            self.prompt_cache_runtime.pinned_cache_edit_refs =
                if self.pinned_cache_edit_refs.is_empty() {
                    cache.pinned_cache_edit_refs
                } else {
                    self.pinned_cache_edit_refs.len() as u32
                };
            self.prompt_cache_runtime.prompt_cache_break_count = cache.prompt_cache_break_count;
            self.prompt_cache_runtime.last_prompt_cache_break_reason =
                cache.last_prompt_cache_break_reason;
            self.prompt_cache_runtime.last_prompt_cache_break_at = cache.last_prompt_cache_break_at;
            self.prompt_cache_runtime
                .last_prompt_cache_expected_drop_reason =
                cache.last_prompt_cache_expected_drop_reason;
            self.prompt_cache_runtime.last_prompt_cache_change_summary =
                cache.last_prompt_cache_change_summary;
            self.prompt_cache_runtime.last_prompt_cache_transition_kind =
                cache.last_prompt_cache_transition_kind;
            self.prompt_cache_runtime
                .last_prompt_cache_transition_reason = cache.last_prompt_cache_transition_reason;
            self.prompt_cache_runtime
                .last_prompt_cache_diff_artifact_path =
                cache.last_prompt_cache_diff_artifact_path.or_else(|| {
                    let path =
                        prompt_cache_diff_artifact_path(&project_root, &self.context.session_id);
                    path.exists().then(|| path.display().to_string())
                });
            self.prompt_cache_runtime.last_prompt_cache_diff_summary =
                cache.last_prompt_cache_diff_summary;
            self.prompt_cache_runtime.reported_turns = cache.reported_turns;
            self.prompt_cache_runtime.cache_write_turns = cache.cache_write_turns;
            self.prompt_cache_runtime.cache_read_turns = cache.cache_read_turns;
            self.prompt_cache_runtime.cache_edit_turns = cache.cache_edit_turns;
            self.prompt_cache_runtime.cache_write_tokens_total = cache.cache_write_tokens_total;
            self.prompt_cache_runtime.cache_read_tokens_total = cache.cache_read_tokens_total;
            self.prompt_cache_runtime.cache_edit_deletions_total = cache.cache_edit_deletions_total;
            self.prompt_cache_runtime.cache_deleted_tokens_total = cache.cache_deleted_tokens_total;

            self.last_prompt_cache_prefix_hash = cache.last_prompt_cache_prefix_hash;
            self.last_prompt_cache_system_hash = cache.last_prompt_cache_system_hash;
            self.last_prompt_cache_restore_hash = cache.last_prompt_cache_restore_hash;
            self.last_prompt_cache_tool_hash = cache.last_prompt_cache_tool_hash;
            self.last_prompt_cache_message_hash = cache.last_prompt_cache_message_hash;
        }

        if let Some(reason) = self.forced_prompt_cache_expected_drop_reason.clone() {
            self.prompt_cache_runtime
                .last_prompt_cache_expected_drop_reason = Some(reason);
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
        let base = self
            .messages
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
            .sum::<usize>();
        let restore = self
            .request_restore_system_blocks()
            .iter()
            .map(|block| block.kind.len().saturating_add(block.content.len()))
            .sum::<usize>();
        base.saturating_add(restore)
    }
}
