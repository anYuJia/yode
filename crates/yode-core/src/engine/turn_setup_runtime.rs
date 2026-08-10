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
        self.record_turn_user_input_with_images(user_input, Vec::new());
    }

    pub(super) fn record_turn_user_input_with_images(
        &mut self,
        user_input: &str,
        images: Vec<yode_llm::types::ImageData>,
    ) {
        if images.is_empty() {
            self.messages.push(Message::user(user_input));
            let persisted_id = self.persist_message("user", Some(user_input), None, None, None);
            self.attach_last_persisted_id(persisted_id);
        } else {
            self.messages
                .push(Message::user_with_images(user_input, images.clone()));
            let persisted_id = self.persist_message_with_images(
                "user",
                Some(user_input),
                None,
                None,
                None,
                Some(&images),
            );
            self.attach_last_persisted_id(persisted_id);
        }
    }

    pub(super) fn reset_turn_runtime_state(&mut self) {
        self.current_turn_started_at = Some(std::time::Instant::now());
        self.reset_stream_watchdog_state();
        self.reset_tool_turn_runtime();
        self.reset_prompt_cache_turn_runtime();
        self.reactive_compact_attempted = false;
        self.reactive_media_strip_attempted = false;
        self.stop_hook_continue_attempted = false;
        self.recent_tool_calls.clear();
        self.consecutive_failures = 0;
        self.violation_retries = 0;
        self.files_modified.clear();
        // BUDGET-001：每轮硬预算状态重置（步数归零，墙钟从本轮起点计时）。
        self.turn_step_count = 0;
        self.turn_budget_started_at = Some(std::time::Instant::now());
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

    /// 配置单轮硬预算（BUDGET-001）。0 表示该维度不设上限。
    pub fn set_turn_budget(&mut self, max_tool_calls: u32, max_steps: u32, max_wall_secs: u64) {
        self.turn_budget_max_tool_calls = max_tool_calls;
        self.turn_budget_max_steps = max_steps;
        self.turn_budget_max_wall_secs = max_wall_secs;
    }

    /// 检查本轮是否触达任一硬预算；未触达返回 None。
    /// 只在 turn 循环的步边界调用，保证检查点单调且可预测。
    pub(super) fn hard_budget_exhausted_reason(&self) -> Option<String> {
        if self.turn_budget_max_tool_calls > 0
            && self.tool_call_count >= self.turn_budget_max_tool_calls
        {
            return Some(format!(
                "工具调用次数达到上限 {} 次",
                self.turn_budget_max_tool_calls
            ));
        }
        if self.turn_budget_max_steps > 0 && self.turn_step_count >= self.turn_budget_max_steps {
            return Some(format!(
                "LLM 循环步数达到上限 {} 步",
                self.turn_budget_max_steps
            ));
        }
        if self.turn_budget_max_wall_secs > 0 {
            if let Some(started) = self.turn_budget_started_at {
                if started.elapsed().as_secs() >= self.turn_budget_max_wall_secs {
                    return Some(format!(
                        "本轮运行时间达到上限 {} 秒",
                        self.turn_budget_max_wall_secs
                    ));
                }
            }
        }
        None
    }
}
