use super::*;
use crate::permission::bash::{
    destructive_guard_reason, destructive_guard_suggestion, discovery_redirect,
};

impl AgentEngine {
    pub(super) async fn run_pre_execution_guards(
        &mut self,
        tool_call: &ToolCall,
        prepared: &mut PreparedToolExecution,
        working_dir: &str,
    ) -> Option<ToolExecutionOutcome> {
        if let Some(blocked) = self
            .run_pre_tool_use_hook(
                &tool_call.name,
                &tool_call.arguments,
                working_dir,
                &mut prepared.params,
            )
            .await
        {
            return Some(ToolExecutionOutcome {
                tool_call: tool_call.clone(),
                result: blocked,
                started_at: prepared.started_at.clone(),
                duration_ms: 0,
                progress_updates: 0,
                last_progress_message: None,
                parallel_batch: None,
            });
        }

        prepared.refresh_metadata(tool_call);

        self.recovery_gate_outcome(tool_call, &prepared.started_at)
            .or_else(|| self.invalid_path_outcome(tool_call, prepared))
            .or_else(|| self.language_mismatch_outcome(tool_call, prepared))
            .or_else(|| self.unread_file_edit_outcome(tool_call, prepared))
            .or_else(|| self.bash_guard_outcome(tool_call, prepared))
    }

    fn recovery_gate_outcome(
        &self,
        tool_call: &ToolCall,
        started_at: &Option<String>,
    ) -> Option<ToolExecutionOutcome> {
        if self.recovery_state != RecoveryState::ReanchorRequired {
            return None;
        }

        let allow_reanchor_tool = matches!(
            tool_call.name.as_str(),
            "ls" | "glob" | "read_file" | "project_map"
        );
        if allow_reanchor_tool {
            return None;
        }

        Some(Self::immediate_tool_outcome(
            tool_call,
            started_at,
            ToolResult::error_typed(
                format!(
                    "Recovery gate active: '{}' is temporarily blocked until workspace is re-anchored.",
                    tool_call.name
                ),
                ToolErrorType::Validation,
                true,
                Some(
                    "Run a lightweight discovery step first (ls/glob/read_file/project_map), then continue with execution tools."
                        .to_string(),
                ),
            ),
        ))
    }

    fn invalid_path_outcome(
        &self,
        tool_call: &ToolCall,
        prepared: &PreparedToolExecution,
    ) -> Option<ToolExecutionOutcome> {
        let file_path = prepared
            .params
            .get("file_path")
            .and_then(|value| value.as_str())?;
        let reason = invalid_path_reason(file_path)?;
        Some(Self::immediate_tool_outcome(
            tool_call,
            &prepared.started_at,
            ToolResult::error_typed(
                format!(
                    "Security Block: '{}' is an invalid path. {}",
                    file_path, reason
                ),
                ToolErrorType::Validation,
                true,
                Some("Correct the path to a literal, normalized format and try again.".to_string()),
            ),
        ))
    }

    fn language_mismatch_outcome(
        &self,
        tool_call: &ToolCall,
        prepared: &PreparedToolExecution,
    ) -> Option<ToolExecutionOutcome> {
        let reason = self.language_command_mismatch(&tool_call.name, &prepared.params)?;
        Some(Self::immediate_tool_outcome(
            tool_call,
            &prepared.started_at,
            ToolResult::error_typed(
                format!("Command blocked by project gate: {}", reason),
                ToolErrorType::Validation,
                true,
                Some(
                    "Re-anchor with ls/glob/read on the target project root, then run matching build tooling."
                        .to_string(),
                ),
            ),
        ))
    }

    fn unread_file_edit_outcome(
        &self,
        tool_call: &ToolCall,
        prepared: &PreparedToolExecution,
    ) -> Option<ToolExecutionOutcome> {
        if tool_call.name != "edit_file" && tool_call.name != "write_file" {
            return None;
        }

        let file_path = prepared
            .params
            .get("file_path")
            .and_then(|value| value.as_str())?;
        if self.files_read.contains_key(file_path) {
            return None;
        }

        Some(Self::immediate_tool_outcome(
            tool_call,
            &prepared.started_at,
            ToolResult::error_typed(
                format!(
                    "You must read the file '{}' with read_file before editing or overwriting it.",
                    file_path
                ),
                ToolErrorType::Validation,
                true,
                Some(format!(
                    "Call read_file(file_path=\"{}\") first.",
                    file_path
                )),
            ),
        ))
    }

    fn bash_guard_outcome(
        &mut self,
        tool_call: &ToolCall,
        prepared: &PreparedToolExecution,
    ) -> Option<ToolExecutionOutcome> {
        let command = prepared.command_content.as_ref()?;
        let command_lower = command.to_lowercase();

        if let Some(redirect) = discovery_redirect(&command_lower) {
            return Some(Self::immediate_tool_outcome(
                tool_call,
                &prepared.started_at,
                ToolResult::error_typed(
                    format!(
                        "Command blocked: Use the dedicated '{}' tool instead of running '{}' via bash.",
                        redirect.alternative, redirect.command_name
                    ),
                    ToolErrorType::Validation,
                    true,
                    Some(format!(
                        "Running search/discovery via bash is inefficient. Use the '{}' tool for better results and TUI display.",
                        redirect.alternative
                    )),
                ),
            ));
        }

        if CommandClassifier::classify(command) == CommandRiskLevel::Destructive {
            self.last_permission_action = Some("deny".to_string());
            self.last_permission_explanation = Some(destructive_guard_reason().to_string());
            self.write_permission_artifact(
                "destructive_guard",
                &tool_call.name,
                "deny",
                destructive_guard_reason(),
                &prepared.params,
                &prepared.effective_arguments,
                &prepared.original_params,
                &tool_call.arguments,
                prepared.input_changed_by_hook,
            );
            return Some(Self::immediate_tool_outcome(
                tool_call,
                &prepared.started_at,
                ToolResult::error_typed(
                    format!("Command blocked (destructive): {}", command),
                    ToolErrorType::PermissionDeny,
                    false,
                    Some(destructive_guard_suggestion().to_string()),
                ),
            ));
        }

        None
    }
}

fn invalid_path_reason(file_path: &str) -> Option<&'static str> {
    if file_path.contains("..") {
        Some("Path traversal (..) is strictly forbidden for security reasons.")
    } else if file_path.contains('$') || file_path.contains('%') {
        Some(
            "Unexpanded shell variables ($VAR, %VAR%) are not allowed in paths. Use absolute or relative literal paths.",
        )
    } else if file_path.starts_with('~') {
        Some(
            "Tilde (~) is not expanded. Use the full absolute path or a path relative to the current working directory.",
        )
    } else {
        None
    }
}
