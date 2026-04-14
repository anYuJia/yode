use super::*;

impl AgentEngine {
    pub fn runtime_state(&self) -> EngineRuntimeState {
        let tool_pool = self.build_tool_pool_snapshot();
        let shared_status = self
            .shared_memory_status
            .try_lock()
            .ok()
            .map(|state| {
                (
                    state.last_session_memory_update_at.clone(),
                    state.last_session_memory_update_path.clone(),
                    state.last_session_memory_generated_summary,
                    state.session_memory_update_count,
                )
            })
            .unwrap_or((None, None, false, 0));
        let hook_stats = self
            .hook_manager
            .as_ref()
            .map(|mgr| mgr.stats_snapshot())
            .unwrap_or_default();
        let recent_permission_denials = self
            .permissions
            .recent_denials(5)
            .into_iter()
            .map(|entry| {
                format!(
                    "{} x{} (consecutive {}, at {})",
                    entry.tool_name, entry.count, entry.consecutive, entry.last_at
                )
            })
            .collect::<Vec<_>>();
        let (tool_trace_scope, tool_traces) = if self.current_tool_execution_traces.is_empty() {
            (
                "last".to_string(),
                self.last_tool_turn_traces
                    .iter()
                    .map(ToolExecutionTrace::to_view)
                    .collect(),
            )
        } else {
            (
                "current".to_string(),
                self.current_tool_execution_traces
                    .iter()
                    .map(ToolExecutionTrace::to_view)
                    .collect(),
            )
        };
        EngineRuntimeState {
            query_source: format!("{:?}", self.current_query_source),
            autocompact_disabled: self.autocompact_disabled,
            compaction_failures: self.compaction_failures,
            total_compactions: self.total_compactions,
            auto_compactions: self.auto_compactions,
            manual_compactions: self.manual_compactions,
            last_compaction_breaker_reason: self.last_compaction_breaker_reason.clone(),
            context_window_tokens: self.context_manager.context_window(),
            compaction_threshold_tokens: self.context_manager.compression_threshold_tokens(),
            estimated_context_tokens: self
                .context_manager
                .estimate_tokens_for_messages(&self.messages),
            message_count: self.messages.len(),
            live_session_memory_initialized: self.session_memory_initialized,
            live_session_memory_updating: self
                .session_memory_update_in_progress
                .load(Ordering::SeqCst),
            live_session_memory_path: live_session_memory_path(&self.context.working_dir_compat())
                .display()
                .to_string(),
            session_tool_calls_total: self.session_tool_calls_total,
            last_compaction_mode: self.last_compaction_mode.clone(),
            last_compaction_at: self.last_compaction_at.clone(),
            last_compaction_summary_excerpt: self.last_compaction_summary_excerpt.clone(),
            last_compaction_session_memory_path: self.last_compaction_session_memory_path.clone(),
            last_compaction_transcript_path: self.last_compaction_transcript_path.clone(),
            last_session_memory_update_at: shared_status.0,
            last_session_memory_update_path: shared_status.1,
            last_session_memory_generated_summary: shared_status.2,
            session_memory_update_count: shared_status.3,
            tracked_failed_tool_results: self.failed_tool_call_ids.len(),
            hook_total_executions: hook_stats.total_executions,
            hook_timeout_count: hook_stats.timeout_count,
            hook_execution_error_count: hook_stats.execution_error_count,
            hook_nonzero_exit_count: hook_stats.nonzero_exit_count,
            hook_wake_notification_count: hook_stats.wake_notification_count,
            last_hook_failure_event: hook_stats.last_failure_event,
            last_hook_failure_command: hook_stats.last_failure_command,
            last_hook_failure_reason: hook_stats.last_failure_reason,
            last_hook_failure_at: hook_stats.last_failure_at,
            last_hook_timeout_command: hook_stats.last_timeout_command,
            last_compaction_prompt_tokens: self.last_compaction_prompt_tokens,
            avg_compaction_prompt_tokens: (self.compaction_prompt_token_samples > 0).then(|| {
                (self.compaction_prompt_tokens_total / self.compaction_prompt_token_samples as u64)
                    as u32
            }),
            compaction_cause_histogram: self.compaction_cause_histogram.clone(),
            system_prompt_estimated_tokens: self.system_prompt_estimated_tokens,
            system_prompt_segments: self.system_prompt_segments.clone(),
            prompt_cache: self.prompt_cache_runtime.clone(),
            last_turn_duration_ms: self.last_turn_duration_ms,
            last_turn_stop_reason: self.last_turn_stop_reason.clone(),
            last_turn_artifact_path: self.last_turn_artifact_path.clone(),
            last_stream_watchdog_stage: self.last_stream_watchdog_stage.clone(),
            stream_retry_reason_histogram: self.stream_retry_reason_histogram.clone(),
            recovery_state: format!("{:?}", self.recovery_state),
            recovery_single_step_count: self.recovery_single_step_count,
            recovery_reanchor_count: self.recovery_reanchor_count,
            recovery_need_user_guidance_count: self.recovery_need_user_guidance_count,
            last_failed_signature: self.last_failed_signature.clone(),
            recovery_breadcrumbs: self.recovery_breadcrumbs.clone(),
            last_recovery_artifact_path: self.last_recovery_artifact_path.clone(),
            last_permission_tool: self.last_permission_tool.clone(),
            last_permission_action: self.last_permission_action.clone(),
            last_permission_explanation: self.last_permission_explanation.clone(),
            last_permission_artifact_path: self.last_permission_artifact_path.clone(),
            recent_permission_denials,
            tool_pool,
            current_turn_tool_calls: self.tool_call_count,
            current_turn_tool_output_bytes: self.total_tool_results_bytes,
            current_turn_tool_progress_events: self.current_turn_tool_progress_events,
            current_turn_parallel_batches: self.current_turn_parallel_batches,
            current_turn_parallel_calls: self.current_turn_parallel_calls,
            current_turn_max_parallel_batch_size: self.current_turn_max_parallel_batch_size,
            current_turn_truncated_results: self.current_turn_truncated_results,
            current_turn_budget_notice_emitted: self.current_turn_budget_notice_emitted,
            current_turn_budget_warning_emitted: self.current_turn_budget_warning_emitted,
            tool_budget_notice_count: self.tool_budget_notice_count,
            tool_budget_warning_count: self.tool_budget_warning_count,
            last_tool_budget_warning: self.last_tool_budget_warning.clone(),
            tool_progress_event_count: self.tool_progress_event_count,
            last_tool_progress_message: self.last_tool_progress_message.clone(),
            last_tool_progress_tool: self.last_tool_progress_tool.clone(),
            last_tool_progress_at: self.last_tool_progress_at.clone(),
            parallel_tool_batch_count: self.parallel_tool_batch_count,
            parallel_tool_call_count: self.parallel_tool_call_count,
            max_parallel_batch_size: self.max_parallel_batch_size,
            tool_truncation_count: self.tool_truncation_count,
            last_tool_truncation_reason: self.last_tool_truncation_reason.clone(),
            latest_repeated_tool_failure: self.latest_repeated_tool_failure.clone(),
            read_file_history: self.read_file_history_preview(),
            command_tool_duplication_hints: self.command_tool_duplication_hints(),
            last_tool_turn_completed_at: self.last_tool_turn_completed_at.clone(),
            last_tool_turn_artifact_path: self.last_tool_turn_artifact_path.clone(),
            tool_error_type_counts: self.tool_error_type_counts.clone(),
            tool_trace_scope,
            tool_traces,
        }
    }

    fn read_file_history_preview(&self) -> Vec<String> {
        let mut entries = self
            .files_read
            .iter()
            .map(|(path, lines)| format!("{} ({} lines)", path, lines))
            .collect::<Vec<_>>();
        entries.sort();
        entries.into_iter().take(8).collect()
    }

    fn command_tool_duplication_hints(&self) -> Vec<String> {
        self.last_tool_turn_traces
            .iter()
            .chain(self.current_tool_execution_traces.iter())
            .filter(|trace| trace.tool_name == "bash")
            .filter_map(|trace| {
                let summary = trace.metadata_summary.as_deref()?;
                summary
                    .contains("rewrite_suggestion=")
                    .then(|| summary.to_string())
            })
            .take(6)
            .collect()
    }

    pub fn runtime_tasks_snapshot(&self) -> Vec<RuntimeTask> {
        self.runtime_task_store
            .try_lock()
            .ok()
            .map(|store| store.list())
            .unwrap_or_default()
    }

    pub fn runtime_task_snapshot(&self, id: &str) -> Option<RuntimeTask> {
        self.runtime_task_store
            .try_lock()
            .ok()
            .and_then(|store| store.get(id))
    }

    pub fn cancel_runtime_task(&self, id: &str) -> bool {
        self.runtime_task_store
            .try_lock()
            .ok()
            .map(|mut store| store.request_cancel(id))
            .unwrap_or(false)
    }

    pub fn drain_runtime_task_notifications(&self) -> Vec<RuntimeTaskNotification> {
        self.runtime_task_store
            .try_lock()
            .ok()
            .map(|mut store| store.drain_notifications())
            .unwrap_or_default()
    }

    pub fn create_runtime_task(
        &self,
        kind: &str,
        source_tool: &str,
        description: &str,
        output_path: &str,
        transcript_path: Option<String>,
    ) -> Option<RuntimeTask> {
        self.runtime_task_store
            .try_lock()
            .ok()
            .map(|mut store| {
                store
                    .create_with_transcript(
                        kind.to_string(),
                        source_tool.to_string(),
                        description.to_string(),
                        output_path.to_string(),
                        transcript_path,
                    )
                    .0
            })
    }

    pub fn mark_runtime_task_running(&self, id: &str) {
        if let Ok(mut store) = self.runtime_task_store.try_lock() {
            store.mark_running(id);
        }
    }

    pub fn update_runtime_task_progress(&self, id: &str, message: impl Into<String>) {
        if let Ok(mut store) = self.runtime_task_store.try_lock() {
            store.update_progress(id, message.into());
        }
    }

    pub fn mark_runtime_task_completed(&self, id: &str) {
        if let Ok(mut store) = self.runtime_task_store.try_lock() {
            store.mark_completed(id);
        }
    }

    pub fn mark_runtime_task_failed(&self, id: &str, error: impl Into<String>) {
        if let Ok(mut store) = self.runtime_task_store.try_lock() {
            store.mark_failed(id, error.into());
        }
    }

    /// Set channels for the ask_user tool.
    pub fn set_ask_user_channels(
        &mut self,
        tx: mpsc::UnboundedSender<UserQuery>,
        rx: mpsc::UnboundedReceiver<String>,
    ) {
        self.ask_user_tx = Some(tx);
        self.ask_user_rx = Some(Arc::new(Mutex::new(rx)));
    }

    /// Build a ToolContext with access to shared resources.
    pub(super) async fn build_tool_context(
        &self,
        progress_tx: Option<mpsc::UnboundedSender<yode_tools::tool::ToolProgress>>,
    ) -> ToolContext {
        let cwd = self.context.runtime.lock().await.cwd.clone();
        let tool_pool_snapshot = self.build_tool_pool_snapshot();

        ToolContext {
            registry: Some(Arc::clone(&self.tools)),
            tasks: Some(Arc::clone(&self.task_store)),
            runtime_tasks: Some(Arc::clone(&self.runtime_task_store)),
            user_input_tx: self.ask_user_tx.clone(),
            user_input_rx: self.ask_user_rx.clone(),
            progress_tx,
            working_dir: Some(cwd),
            sub_agent_runner: Some(Arc::new(SubAgentRunnerImpl {
                provider: Arc::clone(&self.provider),
                tools: Arc::clone(&self.tools),
                context: self.context.clone(),
                runtime_tasks: Arc::clone(&self.runtime_task_store),
            })),
            mcp_resources: None,
            cron_manager: None,
            lsp_manager: None,
            worktree_state: None,
            read_file_history: Some(Arc::new(tokio::sync::Mutex::new(
                std::collections::HashSet::new(),
            ))),
            plan_mode: Some(Arc::clone(&self.plan_mode)),
            tool_pool_snapshot: Some(tool_pool_snapshot),
        }
    }

    pub(super) async fn current_runtime_working_dir(&self) -> String {
        self.context.runtime.lock().await.cwd.display().to_string()
    }

    pub(super) fn parse_tool_input(arguments: &str) -> Value {
        serde_json::from_str(arguments).unwrap_or_else(|_| Value::Object(Map::new()))
    }
}
