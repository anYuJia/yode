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
            ask_user_tx: None,
            ask_user_rx: None,
            tool_call_count: 0,
            recent_tool_calls: Vec::new(),
            consecutive_failures: 0,
            total_tool_results_bytes: 0,
            violation_retries: 0,
            context_manager,
            cost_tracker,
            hook_manager: None,
            files_read: std::collections::HashMap::new(),
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
            compaction_failures: 0,
            total_compactions: 0,
            auto_compactions: 0,
            manual_compactions: 0,
            last_compaction_breaker_reason: None,
            autocompact_disabled: false,
            compaction_in_progress: false,
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
            last_compaction_prompt_tokens: None,
            compaction_prompt_tokens_total: 0,
            compaction_prompt_token_samples: 0,
            compaction_cause_histogram: BTreeMap::new(),
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
        self.reset_autocompact_state();
        self.rebuild_system_prompt();
    }

    pub fn set_provider(&mut self, provider: Arc<dyn LlmProvider>, name: String) {
        self.provider = provider;
        self.context.provider = name;
        self.rebuild_system_prompt();
    }

    pub fn set_effort(&mut self, level: EffortLevel) {
        self.context.effort = level;
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
