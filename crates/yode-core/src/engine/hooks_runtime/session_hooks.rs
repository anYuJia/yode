use super::*;

impl AgentEngine {
    pub async fn initialize_session_hooks(&mut self, reason: &'static str) {
        let Some(hook_mgr) = self.hook_manager.as_ref() else {
            return;
        };

        let runtime_metadata = self.runtime_hook_metadata();
        let hook_ctx = HookContext {
            event: HookEvent::SessionStart.to_string(),
            session_id: self.context.session_id.clone(),
            working_dir: self.context.working_dir_compat().display().to_string(),
            tool_name: None,
            tool_input: None,
            tool_output: None,
            error: None,
            user_prompt: None,
            metadata: Some(json!({
                "reason": reason,
                "resumed": self.context.is_resumed,
                "runtime": runtime_metadata,
            })),
        };

        let results = hook_mgr.execute(HookEvent::SessionStart, &hook_ctx).await;
        let mut combined = String::new();

        for result in results {
            if result.blocked {
                warn!(
                    "session_start hook requested a block, but Yode will continue: {}",
                    result.reason.unwrap_or_default()
                );
            }

            if let Some(stdout) = result.stdout {
                let trimmed = stdout.trim();
                if !trimmed.is_empty() {
                    combined.push_str(trimmed);
                    combined.push_str("\n\n");
                }
            }
        }

        if !combined.is_empty() {
            let message = format!(
                "[System Auto-Context via session_start hooks]\n{}",
                combined
            );
            self.messages.push(Message::system(&message));
            self.persist_message("system", Some(&message), None, None, None);
        }

        self.append_hook_wake_notifications_as_system_message();
    }

    pub async fn finalize_session_hooks(&mut self, reason: &'static str) {
        self.flush_live_session_memory_on_shutdown();

        let Some(_hook_mgr) = self.hook_manager.as_ref() else {
            return;
        };

        let runtime_metadata = self.runtime_hook_metadata();
        let memory_flush_metadata = self.session_end_memory_flush_metadata();
        let hook_ctx = HookContext {
            event: HookEvent::SessionEnd.to_string(),
            session_id: self.context.session_id.clone(),
            working_dir: self.context.working_dir_compat().display().to_string(),
            tool_name: None,
            tool_input: None,
            tool_output: None,
            error: None,
            user_prompt: None,
            metadata: Some(json!({
                "reason": reason,
                "resumed": self.context.is_resumed,
                "total_messages": self.messages.len(),
                "total_tool_calls": self.session_tool_calls_total,
                "runtime": runtime_metadata,
                "memory_flush": memory_flush_metadata,
            })),
        };

        self.execute_advisory_hooks(HookEvent::SessionEnd, hook_ctx)
            .await;
    }

    pub(in crate::engine) fn build_compaction_hook_context(
        &self,
        event: HookEvent,
        mode: &'static str,
        prompt_tokens: u32,
        report: Option<&CompressionReport>,
        session_memory_path: Option<&std::path::Path>,
        transcript_path: Option<&std::path::Path>,
    ) -> HookContext {
        let mut metadata = Map::new();
        metadata.insert("mode".to_string(), json!(mode));
        metadata.insert("prompt_tokens".to_string(), json!(prompt_tokens));
        metadata.insert("message_count".to_string(), json!(self.messages.len()));
        metadata.insert("files_read".to_string(), json!(self.files_read.len()));
        metadata.insert(
            "files_modified".to_string(),
            json!(self.files_modified.len()),
        );
        metadata.insert(
            "runtime".to_string(),
            Value::Object(self.runtime_hook_metadata()),
        );

        if let Some(report) = report {
            metadata.insert("removed".to_string(), json!(report.removed));
            metadata.insert(
                "tool_results_truncated".to_string(),
                json!(report.tool_results_truncated),
            );
            if let Some(summary) = report.summary.as_deref() {
                metadata.insert("summary".to_string(), json!(summary));
            }
        }

        if let Some(path) = session_memory_path {
            metadata.insert(
                "session_memory_path".to_string(),
                json!(path.display().to_string()),
            );
        }

        if let Some(path) = transcript_path {
            metadata.insert(
                "transcript_path".to_string(),
                json!(path.display().to_string()),
            );
        }

        HookContext {
            event: event.to_string(),
            session_id: self.context.session_id.clone(),
            working_dir: self.context.working_dir_compat().display().to_string(),
            tool_name: None,
            tool_input: None,
            tool_output: None,
            error: None,
            user_prompt: None,
            metadata: Some(Value::Object(metadata)),
        }
    }

    pub(in crate::engine) fn runtime_hook_metadata(&self) -> Map<String, Value> {
        let mut metadata = Map::new();
        metadata.insert(
            "compaction_failures".to_string(),
            json!(self.compaction_failures),
        );
        metadata.insert(
            "total_compactions".to_string(),
            json!(self.total_compactions),
        );
        metadata.insert("auto_compactions".to_string(), json!(self.auto_compactions));
        metadata.insert(
            "manual_compactions".to_string(),
            json!(self.manual_compactions),
        );
        metadata.insert(
            "last_compaction_breaker_reason".to_string(),
            self.last_compaction_breaker_reason
                .as_ref()
                .map(|reason| json!(reason))
                .unwrap_or(Value::Null),
        );
        metadata.insert(
            "last_compaction_prompt_tokens".to_string(),
            self.last_compaction_prompt_tokens
                .map(|value| json!(value))
                .unwrap_or(Value::Null),
        );
        metadata.insert(
            "compaction_cause_histogram".to_string(),
            json!(self.compaction_cause_histogram),
        );
        metadata.insert(
            "live_session_memory_initialized".to_string(),
            json!(self.session_memory_initialized),
        );
        metadata.insert(
            "live_session_memory_updating".to_string(),
            json!(self
                .session_memory_update_in_progress
                .load(Ordering::SeqCst)),
        );
        metadata.insert(
            "live_session_memory_path".to_string(),
            json!(live_session_memory_path(&self.context.working_dir_compat())
                .display()
                .to_string()),
        );
        metadata.insert(
            "tracked_failed_tool_results".to_string(),
            json!(self.failed_tool_call_ids.len()),
        );
        metadata.insert(
            "recovery_state".to_string(),
            json!(format!("{:?}", self.recovery_state)),
        );
        metadata.insert(
            "recovery_single_step_count".to_string(),
            json!(self.recovery_single_step_count),
        );
        metadata.insert(
            "recovery_reanchor_count".to_string(),
            json!(self.recovery_reanchor_count),
        );
        metadata.insert(
            "recovery_need_user_guidance_count".to_string(),
            json!(self.recovery_need_user_guidance_count),
        );
        metadata.insert(
            "last_failed_signature".to_string(),
            self.last_failed_signature
                .as_ref()
                .map(|value| json!(value))
                .unwrap_or(Value::Null),
        );

        if let Some(shared) = self.shared_memory_status.try_lock().ok() {
            metadata.insert(
                "last_session_memory_update_at".to_string(),
                shared
                    .last_session_memory_update_at
                    .as_ref()
                    .map(|value| json!(value))
                    .unwrap_or(Value::Null),
            );
            metadata.insert(
                "last_session_memory_update_path".to_string(),
                shared
                    .last_session_memory_update_path
                    .as_ref()
                    .map(|value| json!(value))
                    .unwrap_or(Value::Null),
            );
            metadata.insert(
                "last_session_memory_generated_summary".to_string(),
                json!(shared.last_session_memory_generated_summary),
            );
            metadata.insert(
                "session_memory_update_count".to_string(),
                json!(shared.session_memory_update_count),
            );
        }

        metadata
    }

    pub(super) fn session_end_memory_flush_metadata(&self) -> Value {
        if let Some(shared) = self.shared_memory_status.try_lock().ok() {
            json!({
                "path": shared.last_session_memory_update_path,
                "updated_at": shared.last_session_memory_update_at,
                "generated_summary": shared.last_session_memory_generated_summary,
                "update_count": shared.session_memory_update_count,
            })
        } else {
            Value::Null
        }
    }
}
