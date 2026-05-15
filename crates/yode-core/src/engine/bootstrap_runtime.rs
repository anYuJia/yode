use super::*;

impl AgentEngine {
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        tools: Arc<ToolRegistry>,
        permissions: PermissionManager,
        context: AgentContext,
    ) -> Self {
        let system_prompt_build = Self::build_system_prompt_for_context(&context);
        let system_prompt = system_prompt_build.prompt.clone();

        let messages = vec![Message::system(&system_prompt)];
        let context_manager = ContextManager::new(&context.model);
        let cost_tracker = CostTracker::new(&context.model);
        let detected_project_kind = Self::detect_project_kind(&context.working_dir_compat());

        Self {
            provider,
            tools,
            permissions,
            context,
            messages,
            system_prompt,
            db: None,
            task_store: Arc::new(Mutex::new(TaskStore::new())),
            runtime_task_store: Arc::new(Mutex::new(RuntimeTaskStore::new())),
            team_runtime_manager: Arc::new(Mutex::new(AgentTeamManager::new())),
            worktree_state: Arc::new(Mutex::new(WorktreeState::default())),
            ask_user_tx: None,
            ask_user_rx: None,
            mcp_resource_provider: None,
            tool_call_count: 0,
            recent_tool_calls: Vec::new(),
            consecutive_failures: 0,
            total_tool_results_bytes: 0,
            violation_retries: 0,
            context_manager,
            cost_tracker,
            hook_manager: None,
            files_read: std::collections::HashMap::new(),
            recent_file_reads: Vec::new(),
            files_modified: Vec::new(),
            tool_progress_event_count: 0,
            current_turn_tool_progress_events: 0,
            last_tool_progress_message: None,
            last_tool_progress_tool: None,
            last_tool_progress_at: None,
            parallel_tool_batch_count: 0,
            current_turn_parallel_batches: 0,
            parallel_tool_call_count: 0,
            current_turn_parallel_calls: 0,
            max_parallel_batch_size: 0,
            current_turn_max_parallel_batch_size: 0,
            tool_budget_notice_count: 0,
            tool_budget_warning_count: 0,
            current_turn_budget_notice_emitted: false,
            current_turn_budget_warning_emitted: false,
            last_tool_budget_warning: None,
            tool_truncation_count: 0,
            current_turn_truncated_results: 0,
            last_tool_truncation_reason: None,
            tool_error_type_counts: BTreeMap::new(),
            repeated_tool_failure_patterns: HashMap::new(),
            latest_repeated_tool_failure: None,
            tool_turn_counter: 0,
            current_tool_turn_started_at: None,
            last_tool_turn_completed_at: None,
            current_tool_execution_traces: Vec::new(),
            last_tool_turn_traces: Vec::new(),
            last_tool_turn_artifact_path: None,
            error_buckets: std::collections::HashMap::new(),
            last_failed_signature: None,
            recovery_single_step_count: 0,
            recovery_reanchor_count: 0,
            recovery_need_user_guidance_count: 0,
            recovery_breadcrumbs: Vec::new(),
            last_recovery_artifact_path: None,
            last_permission_tool: None,
            last_permission_action: None,
            last_permission_explanation: None,
            last_permission_artifact_path: None,
            failed_tool_call_ids: HashSet::new(),
            plan_mode: Arc::new(Mutex::new(false)),
            project_kind: detected_project_kind,
            recovery_state: RecoveryState::Normal,
            current_query_source: QuerySource::User,
            current_turn_started_at: None,
            last_turn_duration_ms: None,
            last_turn_stop_reason: None,
            last_turn_artifact_path: None,
            last_stream_watchdog_stage: None,
            stream_retry_reason_histogram: BTreeMap::new(),
            compaction_failures: 0,
            total_compactions: 0,
            auto_compactions: 0,
            manual_compactions: 0,
            last_compaction_breaker_reason: None,
            autocompact_disabled: false,
            compaction_in_progress: false,
            reactive_compact_attempted: false,
            reactive_media_strip_attempted: false,
            stop_hook_continue_attempted: false,
            stop_hook_continue_count: 0,
            last_stop_hook_continue_reason: None,
            cached_microcompact_deleted_refs: Vec::new(),
            pending_cache_edit_refs: Vec::new(),
            pinned_cache_edit_refs: Vec::new(),
            post_compact_restore_blocks: Vec::new(),
            pending_prompt_cache_prefix_hash: None,
            last_prompt_cache_prefix_hash: None,
            pending_prompt_cache_system_hash: None,
            pending_prompt_cache_restore_hash: None,
            pending_prompt_cache_tool_hash: None,
            pending_prompt_cache_message_hash: None,
            last_prompt_cache_system_hash: None,
            last_prompt_cache_restore_hash: None,
            last_prompt_cache_tool_hash: None,
            last_prompt_cache_message_hash: None,
            pending_prompt_cache_system_text: None,
            pending_prompt_cache_restore_text: None,
            pending_prompt_cache_tool_text: None,
            pending_prompt_cache_message_text: None,
            last_prompt_cache_system_text: None,
            last_prompt_cache_restore_text: None,
            last_prompt_cache_tool_text: None,
            last_prompt_cache_message_text: None,
            pending_prompt_cache_expected_drop_reason: None,
            forced_prompt_cache_expected_drop_reason: None,
            session_tool_calls_total: 0,
            session_memory_initialized: false,
            last_session_memory_char_count: 0,
            last_session_memory_tool_count: 0,
            session_memory_update_in_progress: Arc::new(AtomicBool::new(false)),
            session_memory_generation: Arc::new(AtomicU64::new(0)),
            shared_memory_status: Arc::new(Mutex::new(SharedMemoryStatus::default())),
            last_compaction_mode: None,
            last_compaction_at: None,
            last_compaction_summary_excerpt: None,
            last_compaction_session_memory_path: None,
            last_compaction_transcript_path: None,
            last_compact_boundary: None,
            last_compaction_prompt_tokens: None,
            last_post_compaction_estimated_tokens: None,
            last_post_compaction_threshold_tokens: None,
            last_post_compaction_will_retrigger: None,
            compaction_prompt_tokens_total: 0,
            compaction_prompt_token_samples: 0,
            compaction_cause_histogram: BTreeMap::new(),
            last_microcompact_media_removed: 0,
            last_microcompact_media_saved_chars: 0,
            microcompact_media_removed_total: 0,
            microcompact_media_saved_chars_total: 0,
            prompt_cache_runtime: PromptCacheRuntimeState::default(),
            system_prompt_estimated_tokens: system_prompt_build.estimated_tokens,
            system_prompt_segments: system_prompt_build.segments,
        }
    }

    pub fn set_database(&mut self, db: Database) {
        self.db = Some(db);
    }

    pub fn set_model(&mut self, model: String) {
        self.context.model = model;
        self.context_manager = ContextManager::new(&self.context.model);
        self.set_expected_prompt_cache_drop_reason("model_change");
        self.reset_autocompact_state();
        self.rebuild_system_prompt();
    }

    pub fn set_provider(&mut self, provider: Arc<dyn LlmProvider>, name: String) {
        self.provider = provider;
        self.context.provider = name;
        self.set_expected_prompt_cache_drop_reason("provider_change");
        self.rebuild_system_prompt();
    }

    pub fn set_effort(&mut self, level: EffortLevel) {
        self.context.effort = level;
    }

    pub fn set_output_style(&mut self, style: String) {
        if self.context.output_style == style {
            return;
        }
        self.context.output_style = style;
        self.set_expected_prompt_cache_drop_reason("output_style_change");
        self.rebuild_system_prompt();
    }

    pub fn effort(&self) -> EffortLevel {
        self.context.effort
    }

    pub fn current_model(&self) -> &str {
        &self.context.model
    }

    pub fn current_provider(&self) -> &str {
        &self.context.provider
    }

    pub fn permissions(&self) -> &PermissionManager {
        &self.permissions
    }

    pub fn permissions_mut(&mut self) -> &mut PermissionManager {
        &mut self.permissions
    }

    pub fn set_runtime_plan_mode(&self, enabled: bool) -> bool {
        match self.plan_mode.try_lock() {
            Ok(mut mode) => {
                *mode = enabled;
                true
            }
            Err(_) => false,
        }
    }

    pub fn cost_tracker(&self) -> &CostTracker {
        &self.cost_tracker
    }

    pub fn cost_tracker_mut(&mut self) -> &mut CostTracker {
        &mut self.cost_tracker
    }

    pub fn get_database(&self) -> Option<&Database> {
        self.db.as_ref()
    }
}
