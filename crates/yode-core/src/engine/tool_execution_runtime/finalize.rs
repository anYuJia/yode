use super::*;

impl AgentEngine {
    /// Tracks file read/modified status from tool results.
    fn track_file_access(&mut self, tool_name: &str, result: &ToolResult) {
        if result.is_error {
            return;
        }

        if let Some(ref metadata) = result.metadata {
            if let Some(new_cwd) = metadata.get("cwd").and_then(|v| v.as_str()) {
                let runtime = self.context.runtime.clone();
                let new_path = std::path::PathBuf::from(new_cwd);
                tokio::spawn(async move {
                    let mut rt = runtime.lock().await;
                    if rt.cwd != new_path {
                        debug!("Syncing session CWD to: {}", new_path.display());
                        rt.cwd = new_path.clone();
                        rt.last_success_cwd = new_path;
                    }
                });
            }

            if let Some(file_path) = metadata.get("file_path").and_then(|v| v.as_str()) {
                match tool_name {
                    "read_file" => {
                        let lines = metadata
                            .get("total_lines")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as usize;
                        self.files_read.insert(file_path.to_string(), lines);
                    }
                    "edit_file" | "write_file" | "multi_edit" | "notebook_edit" => {
                        self.files_modified.push(file_path.to_string());
                    }
                    _ => {}
                }
            }
        }
    }

    /// Enforces per-turn aggregate tool result size limits.
    fn enforce_tool_budget(&mut self, result: &mut ToolResult) {
        let size = result.content.len();
        self.total_tool_results_bytes += size;

        if self.total_tool_results_bytes > MAX_TOTAL_TOOL_RESULTS_SIZE {
            let over_limit = self.total_tool_results_bytes - MAX_TOTAL_TOOL_RESULTS_SIZE;
            if size > over_limit {
                let allowed = size.saturating_sub(over_limit);
                let preview_len = allowed.min(1000);
                let preview: String = result.content.chars().take(preview_len).collect();
                let original_bytes = size;

                result.content = format!(
                    "{}\n\n[AGGREGATE BUDGET EXCEEDED: Remaining {} bytes of this result omitted. \
                     STOP requesting large outputs in this turn to avoid context overflow.]",
                    preview,
                    size - preview_len
                );
                set_tool_runtime_truncation_metadata(
                    result,
                    &ToolResultTruncationView {
                        reason: "aggregate_budget_partial".to_string(),
                        original_bytes,
                        kept_bytes: result.content.len(),
                        omitted_bytes: original_bytes.saturating_sub(result.content.len()),
                    },
                );
            } else {
                let original_bytes = size;
                result.content = format!(
                    "[AGGREGATE BUDGET EXCEEDED: Full result ({} bytes) omitted to prevent context overflow. \
                     Summarize your current findings instead.]",
                    size
                );
                set_tool_runtime_truncation_metadata(
                    result,
                    &ToolResultTruncationView {
                        reason: "aggregate_budget_omitted".to_string(),
                        original_bytes,
                        kept_bytes: result.content.len(),
                        omitted_bytes: original_bytes.saturating_sub(result.content.len()),
                    },
                );
            }
        }
    }

    pub(in crate::engine) async fn finalize_tool_result(
        &mut self,
        tool_call: &ToolCall,
        mut result: ToolResult,
        started_at: Option<String>,
        duration_ms: u64,
        progress_updates: u32,
        parallel_batch: Option<u32>,
    ) -> ToolResult {
        self.track_file_access(&tool_call.name, &result);
        result = truncate_tool_result(result);
        self.enforce_tool_budget(&mut result);
        self.inject_intelligence(&mut result, &tool_call.name, &tool_call.arguments);

        let working_dir = self.current_runtime_working_dir().await;
        let effective_input = Self::parse_tool_input(&tool_call.arguments);
        self.run_post_tool_use_hooks(tool_call, &effective_input, &working_dir, &mut result)
            .await;
        self.emit_tool_specific_lifecycle_hooks(tool_call, &working_dir, &result)
            .await;
        annotate_tool_result_runtime_metadata(
            &mut result,
            duration_ms,
            progress_updates,
            parallel_batch,
            tool_call.arguments.len(),
        );

        self.record_tool_execution_trace(
            tool_call,
            &result,
            started_at,
            duration_ms,
            progress_updates,
            parallel_batch,
            tool_call.arguments.len(),
        );
        self.record_tool_result_status(&tool_call.id, &result);
        result
    }

    async fn emit_tool_specific_lifecycle_hooks(
        &mut self,
        tool_call: &ToolCall,
        working_dir: &str,
        result: &ToolResult,
    ) {
        if result.is_error {
            return;
        }

        if tool_call.name == "enter_worktree" {
            let metadata = result.metadata.clone().unwrap_or_else(|| json!({}));
            let ctx = HookContext {
                event: HookEvent::WorktreeCreate.to_string(),
                session_id: self.context.session_id.clone(),
                working_dir: working_dir.to_string(),
                tool_name: Some(tool_call.name.clone()),
                tool_input: Some(Self::parse_tool_input(&tool_call.arguments)),
                tool_output: Some(result.content.clone()),
                error: None,
                user_prompt: None,
                metadata: Some(metadata),
            };
            self.execute_advisory_hooks(HookEvent::WorktreeCreate, ctx).await;
        }
    }

    pub(super) fn immediate_tool_outcome(
        tool_call: &ToolCall,
        started_at: &Option<String>,
        result: ToolResult,
    ) -> ToolExecutionOutcome {
        ToolExecutionOutcome {
            tool_call: tool_call.clone(),
            result,
            started_at: started_at.clone(),
            duration_ms: 0,
            progress_updates: 0,
            last_progress_message: None,
            parallel_batch: None,
        }
    }
}
