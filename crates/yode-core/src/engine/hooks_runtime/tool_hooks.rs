use super::*;
use serde_json::json;

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

            if result.deferred {
                let reason = result
                    .reason
                    .clone()
                    .unwrap_or_else(|| format!("pre_tool_use deferred {}", tool_name));
                let original_input = Self::parse_tool_input(tool_arguments);
                let input_changed_by_hook = *params != original_input;
                let effective_arguments_snapshot =
                    serde_json::to_string(params).unwrap_or_else(|_| tool_arguments.to_string());
                let (summary_path, state_path) = self.write_hook_defer_artifact(
                    tool_name,
                    working_dir,
                    params,
                    &original_input,
                    &effective_arguments_snapshot,
                    tool_arguments,
                    input_changed_by_hook,
                    &reason,
                    result.source_hook_command.as_deref(),
                );
                return Some(ToolResult::success_with_metadata(
                    format!(
                        "Deferred by hook: {}.\nSummary: {}\nState: {}\nResume by retrying the tool call after the external action completes.",
                        reason,
                        summary_path.as_deref().unwrap_or("none"),
                        state_path.as_deref().unwrap_or("none"),
                    ),
                    json!({
                        "deferred": true,
                        "defer_reason": reason,
                        "defer_summary_artifact": summary_path,
                        "defer_state_artifact": state_path,
                        "tool_name": tool_name,
                        "source_hook_command": result.source_hook_command,
                        "effective_input_snapshot": params,
                        "original_input_snapshot": original_input,
                        "effective_arguments_snapshot": effective_arguments_snapshot,
                        "original_arguments_snapshot": tool_arguments,
                        "input_changed_by_hook": input_changed_by_hook,
                    }),
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

    fn write_hook_defer_artifact(
        &self,
        tool_name: &str,
        working_dir: &str,
        params: &Value,
        original_input: &Value,
        effective_arguments: &str,
        original_arguments: &str,
        input_changed_by_hook: bool,
        reason: &str,
        source_hook_command: Option<&str>,
    ) -> (Option<String>, Option<String>) {
        let dir = self
            .context
            .working_dir_compat()
            .join(".yode")
            .join("hooks");
        if std::fs::create_dir_all(&dir).is_err() {
            return (None, None);
        }
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let short_session = self.context.session_id.chars().take(8).collect::<String>();
        let summary_path = dir.join(format!("{}-{}-hook-deferred.md", stamp, short_session));
        let state_path = dir.join(format!(
            "{}-{}-hook-deferred-state.json",
            stamp, short_session
        ));
        let payload = json!({
            "kind": "hook_deferred_tool_call",
            "updated_at": Self::now_timestamp(),
            "session_id": self.context.session_id,
            "tool_name": tool_name,
            "working_dir": working_dir,
            "reason": reason,
            "source_hook_command": source_hook_command,
            "effective_input_snapshot": params,
            "original_input_snapshot": original_input,
            "effective_arguments_snapshot": effective_arguments,
            "original_arguments_snapshot": original_arguments,
            "input_changed_by_hook": input_changed_by_hook,
            "resume_hint": format!("Retry tool '{}' after completing the deferred external action.", tool_name),
        });
        let summary = format!(
            "# Hook Deferred Tool Call\n\n- Tool: {}\n- Working dir: {}\n- Reason: {}\n- Source hook: {}\n- Resume hint: Retry tool `{}` after the deferred external action completes.\n- State artifact: {}\n",
            tool_name,
            working_dir,
            reason,
            source_hook_command.unwrap_or("unknown"),
            tool_name,
            state_path.display(),
        );
        let summary_ok = std::fs::write(&summary_path, summary).is_ok();
        let state_ok = std::fs::write(
            &state_path,
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
        )
        .is_ok();
        (
            summary_ok.then(|| summary_path.display().to_string()),
            state_ok.then(|| state_path.display().to_string()),
        )
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
