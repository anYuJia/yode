use super::*;

impl AgentEngine {
    pub(in crate::engine) async fn run_pre_tool_use_hook(
        &self,
        tool_name: &str,
        tool_arguments: &str,
        working_dir: &str,
        params: &mut Value,
    ) -> Option<ToolResult> {
        let hook_mgr = self.hook_manager.as_ref()?;
        let hook_ctx = HookContext {
            event: HookEvent::PreToolUse.to_string(),
            session_id: self.context.session_id.clone(),
            working_dir: working_dir.to_string(),
            tool_name: Some(tool_name.to_string()),
            tool_input: Some(params.clone()),
            tool_output: None,
            error: None,
            user_prompt: None,
            metadata: None,
        };
        let results = hook_mgr.execute(HookEvent::PreToolUse, &hook_ctx).await;
        let mut hook_outputs = Vec::new();

        for result in results {
            if let Some(modified_input) = result.modified_input {
                *params = modified_input;
            }

            if result.blocked {
                return Some(ToolResult::error_typed(
                    format!(
                        "Blocked by hook: {}",
                        result
                            .reason
                            .unwrap_or_else(|| format!("pre_tool_use rejected {}", tool_name))
                    ),
                    ToolErrorType::PermissionDeny,
                    false,
                    None,
                ));
            }

            if let Some(stdout) = result.stdout {
                let trimmed = stdout.trim();
                if !trimmed.is_empty() {
                    hook_outputs.push(trimmed.to_string());
                }
            }
        }

        if !hook_outputs.is_empty() {
            info!(
                "pre_tool_use hook output for {}({}): {}",
                tool_name,
                tool_arguments,
                hook_outputs.join(" | ")
            );
        }

        None
    }

    pub(in crate::engine) async fn run_post_tool_use_hooks(
        &self,
        tool_call: &ToolCall,
        effective_input: &Value,
        working_dir: &str,
        result: &mut ToolResult,
    ) {
        let Some(hook_mgr) = self.hook_manager.as_ref() else {
            return;
        };

        let event = if result.is_error {
            HookEvent::PostToolUseFailure
        } else {
            HookEvent::PostToolUse
        };

        let hook_ctx = HookContext {
            event: event.to_string(),
            session_id: self.context.session_id.clone(),
            working_dir: working_dir.to_string(),
            tool_name: Some(tool_call.name.clone()),
            tool_input: Some(effective_input.clone()),
            tool_output: Some(result.content.clone()),
            error: result.is_error.then(|| result.content.clone()),
            user_prompt: None,
            metadata: result.metadata.clone(),
        };

        let results = hook_mgr.execute(event, &hook_ctx).await;
        let mut hook_outputs = Vec::new();

        for hook_result in results {
            if hook_result.blocked {
                warn!(
                    "Post-tool hook requested block for {}: {}",
                    tool_call.name,
                    hook_result.reason.unwrap_or_default()
                );
            }

            if let Some(stdout) = hook_result.stdout {
                let trimmed = stdout.trim();
                if !trimmed.is_empty() {
                    hook_outputs.push(trimmed.to_string());
                }
            }
        }

        if !hook_outputs.is_empty() {
            result.content.push_str("\n\n[Post-tool hook output]\n");
            result.content.push_str(&hook_outputs.join("\n\n"));
        }
    }

    pub(in crate::engine) async fn execute_advisory_hooks(
        &mut self,
        event: HookEvent,
        context: HookContext,
    ) {
        let Some(hook_mgr) = self.hook_manager.as_ref() else {
            return;
        };

        for hook_result in hook_mgr.execute(event.clone(), &context).await {
            if hook_result.blocked {
                warn!(
                    "{} hook requested a block, but Yode will continue: {}",
                    event,
                    hook_result.reason.unwrap_or_default()
                );
            }

            if let Some(stdout) = hook_result.stdout {
                let trimmed = stdout.trim();
                if !trimmed.is_empty() {
                    info!("{} hook output: {}", event, trimmed);
                }
            }
        }

        self.append_hook_wake_notifications_as_system_message();
    }

    pub(in crate::engine) async fn append_hook_outputs_as_system_message(
        &mut self,
        event: HookEvent,
        context: HookContext,
        banner: &str,
    ) {
        let Some(hook_mgr) = self.hook_manager.as_ref() else {
            return;
        };

        let results = hook_mgr.execute(event.clone(), &context).await;
        let mut combined = String::new();

        for result in results {
            if result.blocked {
                warn!(
                    "{} hook requested a block, but Yode will continue: {}",
                    event,
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
            let message = format!("[{}]\n{}", banner, combined);
            self.messages.push(Message::system(&message));
            self.persist_message("system", Some(&message), None, None, None);
        }

        self.append_hook_wake_notifications_as_system_message();
    }
}
