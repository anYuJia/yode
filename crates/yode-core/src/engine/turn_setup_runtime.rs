use super::*;

impl AgentEngine {
    pub(super) async fn append_turn_setup_context(&mut self, user_input: &str) {
        let prompt_submit_ctx = HookContext {
            event: HookEvent::UserPromptSubmit.to_string(),
            session_id: self.context.session_id.clone(),
            working_dir: self.context.working_dir_compat().display().to_string(),
            tool_name: None,
            tool_input: None,
            tool_output: None,
            error: None,
            user_prompt: Some(user_input.to_string()),
            metadata: Some(json!({
                "query_source": format!("{:?}", self.current_query_source),
            })),
        };
        self.append_hook_outputs_as_system_message(
            HookEvent::UserPromptSubmit,
            prompt_submit_ctx,
            "System Auto-Context via user_prompt_submit hooks",
        )
        .await;

        if let Some(ref hook_mgr) = self.hook_manager {
            let hook_ctx = HookContext {
                event: "pre_turn".into(),
                session_id: self.context.session_id.clone(),
                working_dir: self.context.working_dir_compat().display().to_string(),
                tool_name: None,
                tool_input: None,
                tool_output: None,
                error: None,
                user_prompt: Some(user_input.to_string()),
                metadata: None,
            };
            let results = hook_mgr.execute(HookEvent::PreTurn, &hook_ctx).await;
            let mut combined = String::new();
            for res in results {
                if let Some(out) = res.stdout {
                    combined.push_str(&out);
                    combined.push_str("\n\n");
                }
            }
            if !combined.is_empty() {
                self.messages.push(Message::system(format!(
                    "[System Auto-Context via pre_turn hooks]\n{}",
                    combined
                )));
            }
            self.append_hook_wake_notifications_as_system_message();
        }
    }

    pub(super) fn record_turn_user_input(&mut self, user_input: &str) {
        self.messages.push(Message::user(user_input));
        self.persist_message("user", Some(user_input), None, None, None);
    }

    pub(super) fn reset_turn_runtime_state(&mut self) {
        self.current_turn_started_at = Some(std::time::Instant::now());
        self.reset_stream_watchdog_state();
        self.reset_tool_turn_runtime();
        self.reset_prompt_cache_turn_runtime();
        self.recent_tool_calls.clear();
        self.consecutive_failures = 0;
        self.violation_retries = 0;
        self.files_read.clear();
        self.files_modified.clear();
    }

    pub(super) fn reset_stream_watchdog_state(&mut self) {
        self.last_stream_watchdog_stage = None;
    }

    pub(super) fn reset_non_streaming_error_state(&mut self) {
        self.error_buckets.clear();
        self.last_failed_signature = None;
        self.update_recovery_state();
        self.error_buckets.clear();
        self.last_failed_signature = None;
        self.update_recovery_state();
    }
}
