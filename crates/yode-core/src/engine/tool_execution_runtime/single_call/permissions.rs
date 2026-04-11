use super::*;

impl AgentEngine {
    pub(super) async fn handle_permission_action(
        &mut self,
        action: PermissionAction,
        permission_reason: &str,
        tool_call: &ToolCall,
        prepared: &PreparedToolExecution,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
        confirm_rx: &mut mpsc::UnboundedReceiver<ConfirmResponse>,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<Option<ToolExecutionOutcome>> {
        match action {
            PermissionAction::Allow => {
                info!("Executing tool: {} (auto-allowed)", tool_call.name);
                let _ = event_tx.send(EngineEvent::ToolCallStart {
                    id: tool_call.id.clone(),
                    name: tool_call.name.clone(),
                    arguments: prepared.effective_arguments.clone(),
                });
                Ok(None)
            }
            PermissionAction::Confirm => {
                self.await_tool_confirmation(
                    tool_call,
                    prepared,
                    event_tx,
                    confirm_rx,
                    cancel_token,
                )
                .await
            }
            PermissionAction::Deny => {
                self.handle_permission_denial(tool_call, permission_reason, prepared)
                    .await
            }
        }
    }

    async fn await_tool_confirmation(
        &mut self,
        tool_call: &ToolCall,
        prepared: &PreparedToolExecution,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
        confirm_rx: &mut mpsc::UnboundedReceiver<ConfirmResponse>,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<Option<ToolExecutionOutcome>> {
        self.permissions.record_confirmation_request(
            &tool_call.name,
            prepared.command_content.as_deref(),
        );
        let permission_request_ctx = HookContext {
            event: HookEvent::PermissionRequest.to_string(),
            session_id: self.context.session_id.clone(),
            working_dir: self.context.working_dir_compat().display().to_string(),
            tool_name: Some(tool_call.name.clone()),
            tool_input: Some(prepared.params.clone()),
            tool_output: None,
            error: None,
            user_prompt: None,
            metadata: Some(json!({
                "decision": "confirm",
                "effective_input_snapshot": prepared.params.clone(),
                "effective_arguments_snapshot": prepared.effective_arguments.clone(),
                "original_input_snapshot": prepared.original_params.clone(),
                "original_arguments_snapshot": tool_call.arguments.clone(),
                "input_changed_by_hook": prepared.input_changed_by_hook,
            })),
        };
        self.execute_advisory_hooks(HookEvent::PermissionRequest, permission_request_ctx)
            .await;

        let _ = event_tx.send(EngineEvent::ToolConfirmRequired {
            id: tool_call.id.clone(),
            name: tool_call.name.clone(),
            arguments: prepared.effective_arguments.clone(),
        });

        debug!("Waiting for user confirmation: tool={}", tool_call.name);
        let confirm_start = std::time::Instant::now();
        let confirm_timeout = std::time::Duration::from_secs(90);
        loop {
            if confirm_start.elapsed() > confirm_timeout {
                return Ok(Some(Self::immediate_tool_outcome(
                    tool_call,
                    &prepared.started_at,
                    ToolResult::error_typed(
                        format!("Confirmation timed out for tool '{}'", tool_call.name),
                        ToolErrorType::Timeout,
                        true,
                        Some(
                            "No confirmation was received within 90s. Re-run or switch to a read-only alternative."
                                .to_string(),
                        ),
                    ),
                )));
            }

            if let Some(token) = cancel_token {
                if token.is_cancelled() {
                    return Ok(Some(Self::immediate_tool_outcome(
                        tool_call,
                        &prepared.started_at,
                        ToolResult::error_typed(
                            format!("Tool confirmation cancelled: {}", tool_call.name),
                            ToolErrorType::Timeout,
                            true,
                            Some("User cancelled while waiting for confirmation.".to_string()),
                        ),
                    )));
                }
            }

            match tokio::time::timeout(std::time::Duration::from_millis(500), confirm_rx.recv())
                .await
            {
                Ok(Some(ConfirmResponse::Allow)) => {
                    info!("Tool {} confirmed by user", tool_call.name);
                    break;
                }
                Ok(Some(ConfirmResponse::Deny)) => {
                    info!("Tool {} denied by user", tool_call.name);
                    self.permissions.record_denial(&tool_call.name);
                    self.permissions
                        .record_shell_prefix_denial(prepared.command_content.as_deref());
                    self.write_permission_artifact(
                        "user_confirmation",
                        &tool_call.name,
                        "deny",
                        "Tool execution denied by user.",
                        &prepared.params,
                        &prepared.effective_arguments,
                        &prepared.original_params,
                        &tool_call.arguments,
                        prepared.input_changed_by_hook,
                    );
                    let denied_ctx = HookContext {
                        event: HookEvent::PermissionDenied.to_string(),
                        session_id: self.context.session_id.clone(),
                        working_dir: self.context.working_dir_compat().display().to_string(),
                        tool_name: Some(tool_call.name.clone()),
                        tool_input: Some(prepared.params.clone()),
                        tool_output: None,
                        error: Some("Tool execution denied by user.".to_string()),
                        user_prompt: None,
                        metadata: Some(json!({
                            "source": "user_confirmation",
                            "effective_input_snapshot": prepared.params.clone(),
                            "effective_arguments_snapshot": prepared.effective_arguments.clone(),
                            "original_input_snapshot": prepared.original_params.clone(),
                            "original_arguments_snapshot": tool_call.arguments.clone(),
                            "input_changed_by_hook": prepared.input_changed_by_hook,
                        })),
                    };
                    self.execute_advisory_hooks(HookEvent::PermissionDenied, denied_ctx)
                        .await;
                    return Ok(Some(Self::immediate_tool_outcome(
                        tool_call,
                        &prepared.started_at,
                        ToolResult::error("Tool execution denied by user.".to_string()),
                    )));
                }
                Ok(None) => {
                    return Ok(Some(Self::immediate_tool_outcome(
                        tool_call,
                        &prepared.started_at,
                        ToolResult::error_typed(
                            "Confirmation channel closed.".to_string(),
                            ToolErrorType::Execution,
                            true,
                            Some(
                                "Please retry the action. If this repeats, check TUI confirmation event handling."
                                    .to_string(),
                            ),
                        ),
                    )));
                }
                Err(_) => {}
            }
        }

        Ok(None)
    }

    async fn handle_permission_denial(
        &mut self,
        tool_call: &ToolCall,
        permission_reason: &str,
        prepared: &PreparedToolExecution,
    ) -> Result<Option<ToolExecutionOutcome>> {
        self.permissions
            .record_shell_prefix_denial(prepared.command_content.as_deref());
        let denied_ctx = HookContext {
            event: HookEvent::PermissionDenied.to_string(),
            session_id: self.context.session_id.clone(),
            working_dir: self.context.working_dir_compat().display().to_string(),
            tool_name: Some(tool_call.name.clone()),
            tool_input: Some(prepared.params.clone()),
            tool_output: None,
            error: Some(format!("Tool {} is not permitted.", tool_call.name)),
            user_prompt: None,
            metadata: Some(json!({
                "source": "permission_manager",
                "effective_input_snapshot": prepared.params.clone(),
                "effective_arguments_snapshot": prepared.effective_arguments.clone(),
                "original_input_snapshot": prepared.original_params.clone(),
                "original_arguments_snapshot": tool_call.arguments.clone(),
                "input_changed_by_hook": prepared.input_changed_by_hook,
            })),
        };
        self.execute_advisory_hooks(HookEvent::PermissionDenied, denied_ctx)
            .await;

        Ok(Some(Self::immediate_tool_outcome(
            tool_call,
            &prepared.started_at,
            ToolResult::error_typed(
                format!(
                    "Tool {} is not permitted. {}",
                    tool_call.name, permission_reason
                ),
                ToolErrorType::PermissionDeny,
                false,
                Some(
                    "Use a safer read-only tool first, or switch permission mode / rules explicitly before retrying."
                        .to_string(),
                ),
            ),
        )))
    }
}
