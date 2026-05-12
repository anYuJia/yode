mod execution;
mod guards;
mod permissions;

use super::*;

struct PreparedToolExecution {
    started_at: Option<String>,
    original_params: Value,
    params: Value,
    effective_arguments: String,
    input_changed_by_hook: bool,
    command_content: Option<String>,
}

impl PreparedToolExecution {
    fn new(started_at: Option<String>, original_params: Value) -> Self {
        Self {
            started_at,
            params: original_params.clone(),
            original_params,
            effective_arguments: String::new(),
            input_changed_by_hook: false,
            command_content: None,
        }
    }

    fn refresh_metadata(&mut self, tool_call: &ToolCall) {
        self.command_content = if tool_call.name == "bash" {
            self.params
                .get("command")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        } else {
            None
        };
        self.effective_arguments =
            serde_json::to_string(&self.params).unwrap_or_else(|_| tool_call.arguments.clone());
        self.input_changed_by_hook = self.params != self.original_params;
    }
}

impl AgentEngine {
    /// Handle a single tool call...
    pub(in crate::engine) async fn handle_tool_call(
        &mut self,
        tool_call: &ToolCall,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
        confirm_rx: &mut mpsc::UnboundedReceiver<ConfirmResponse>,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<ToolExecutionOutcome> {
        let started_at = Some(Self::now_timestamp());
        let tool = match self.tools.get(&tool_call.name) {
            Some(tool) => tool,
            None => {
                return Ok(ToolExecutionOutcome {
                    tool_call: tool_call.clone(),
                    result: ToolResult::error(format!("Unknown tool: {}", tool_call.name)),
                    started_at,
                    duration_ms: 0,
                    progress_updates: 0,
                    last_progress_message: None,
                    parallel_batch: None,
                });
            }
        };

        let original_params: Value = serde_json::from_str(&tool_call.arguments)
            .unwrap_or_else(|_| Value::Object(Map::new()));
        let mut prepared = PreparedToolExecution::new(started_at.clone(), original_params);
        let working_dir = self.current_runtime_working_dir().await;

        if let Some(outcome) = self
            .run_pre_execution_guards(tool_call, &mut prepared, &working_dir)
            .await
        {
            return Ok(outcome);
        }

        let mut permission_explanation = self
            .permissions
            .explain_with_content(&tool_call.name, prepared.command_content.as_deref());
        if let Some(reason) = runtime_plan_mode_override_reason(
            *self.plan_mode.lock().await,
            &tool_call.name,
            tool.capabilities(),
        ) {
            permission_explanation.action = PermissionAction::Deny;
            permission_explanation.reason = reason;
            permission_explanation.precedence_chain.insert(
                0,
                "runtime-plan-mode: mutating capability -> deny".to_string(),
            );
        }
        self.last_permission_tool = Some(tool_call.name.clone());
        self.last_permission_action = Some(permission_explanation.action.label().to_string());
        self.last_permission_explanation = Some(permission_explanation.reason.clone());
        self.write_permission_artifact(
            "permission_manager",
            &tool_call.name,
            permission_explanation.action.label(),
            &permission_explanation.reason,
            &prepared.params,
            &prepared.effective_arguments,
            &prepared.original_params,
            &tool_call.arguments,
            prepared.input_changed_by_hook,
        );

        if let Some(outcome) = self
            .handle_permission_action(
                permission_explanation.action.clone(),
                &permission_explanation.reason,
                tool_call,
                &prepared,
                event_tx,
                confirm_rx,
                cancel_token,
            )
            .await?
        {
            return Ok(outcome);
        }

        if let Some(outcome) = self.block_repeated_or_duplicate_call(tool_call, &prepared) {
            return Ok(outcome);
        }

        Ok(self
            .execute_tool_with_tracking(tool_call, &tool, prepared, event_tx)
            .await)
    }
}

fn runtime_plan_mode_override_reason(
    enabled: bool,
    tool_name: &str,
    capabilities: yode_tools::tool::ToolCapabilities,
) -> Option<String> {
    if !enabled || tool_name == "exit_plan_mode" || capabilities.read_only {
        return None;
    }

    Some(format!(
        "Runtime plan mode blocks mutating tools based on tool annotations. Use read-only exploration or exit plan mode before running '{}'.",
        tool_name
    ))
}

#[cfg(test)]
mod tests {
    use super::runtime_plan_mode_override_reason;
    use yode_tools::tool::ToolCapabilities;

    #[test]
    fn runtime_plan_mode_blocks_mutating_tools_by_capability() {
        let reason = runtime_plan_mode_override_reason(
            true,
            "write_file",
            ToolCapabilities {
                read_only: false,
                requires_confirmation: true,
                supports_auto_execution: false,
            },
        );

        assert!(reason
            .as_deref()
            .is_some_and(|text| text.contains("blocks mutating tools")));
    }

    #[test]
    fn runtime_plan_mode_allows_readonly_and_exit_plan_mode() {
        assert!(runtime_plan_mode_override_reason(
            true,
            "read_file",
            ToolCapabilities {
                read_only: true,
                requires_confirmation: false,
                supports_auto_execution: true,
            },
        )
        .is_none());
        assert!(runtime_plan_mode_override_reason(
            true,
            "exit_plan_mode",
            ToolCapabilities::default()
        )
        .is_none());
    }
}
