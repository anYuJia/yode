use super::*;

impl AgentEngine {
    pub(super) fn block_repeated_or_duplicate_call(
        &mut self,
        tool_call: &ToolCall,
        prepared: &PreparedToolExecution,
    ) -> Option<ToolExecutionOutcome> {
        let call_signature = (tool_call.name.clone(), prepared.effective_arguments.clone());
        let current_signature_text = format!("{}:{}", tool_call.name, prepared.effective_arguments);

        if !is_observer_tool(&tool_call.name)
            && self.consecutive_failures >= 2
            && self.last_failed_signature.as_ref() == Some(&current_signature_text)
        {
            return Some(Self::immediate_tool_outcome(
                tool_call,
                &prepared.started_at,
                ToolResult::error_typed(
                    format!(
                        "Blocked repeated failing call: {} is being retried with identical arguments after multiple failures.",
                        tool_call.name
                    ),
                    ToolErrorType::Validation,
                    true,
                    Some(
                        "Do not retry the same call. Re-anchor first (ls/glob/read), then change tool arguments."
                            .to_string(),
                    ),
                ),
            ));
        }

        if self.recent_tool_calls.contains(&call_signature) && !is_observer_tool(&tool_call.name) {
            return Some(Self::immediate_tool_outcome(
                tool_call,
                &prepared.started_at,
                ToolResult::error_typed(
                    format!(
                        "Duplicate tool call detected: {} was called with identical arguments recently. \
                         If you are stuck, try a different approach, search for more information, or summarize your progress.",
                        tool_call.name
                    ),
                    ToolErrorType::Validation,
                    true,
                    Some(
                        "Do NOT resend identical tool parameters. Re-anchor with a lightweight read/list action, then adjust arguments."
                            .to_string(),
                    ),
                ),
            ));
        }

        self.recent_tool_calls.push(call_signature);
        if self.recent_tool_calls.len() > 10 {
            self.recent_tool_calls.remove(0);
        }

        None
    }

    pub(super) async fn execute_tool_with_tracking(
        &mut self,
        tool_call: &ToolCall,
        tool: &Arc<dyn yode_tools::tool::Tool>,
        mut prepared: PreparedToolExecution,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) -> ToolExecutionOutcome {
        self.cost_tracker.record_tool_call();
        debug!(
            "Tool execute start: tool={} args={}",
            tool_call.name, tool_call.arguments
        );
        let start_time = std::time::Instant::now();

        let (progress_tx, mut progress_rx) =
            mpsc::unbounded_channel::<yode_tools::tool::ToolProgress>();
        let event_tx_inner = event_tx.clone();
        let tool_call_id = tool_call.id.clone();
        let tool_name = tool_call.name.clone();
        let progress_counter = Arc::new(AtomicU64::new(0));
        let progress_counter_inner = Arc::clone(&progress_counter);
        let last_progress_message = Arc::new(std::sync::Mutex::new(None::<String>));
        let last_progress_message_inner = Arc::clone(&last_progress_message);

        tokio::spawn(async move {
            while let Some(progress) = progress_rx.recv().await {
                progress_counter_inner.fetch_add(1, Ordering::Relaxed);
                if let Ok(mut slot) = last_progress_message_inner.lock() {
                    *slot = Some(progress.message.clone());
                }
                let _ = event_tx_inner.send(EngineEvent::ToolProgress {
                    id: tool_call_id.clone(),
                    name: tool_name.clone(),
                    progress,
                });
            }
        });

        let ctx = self.build_tool_context(Some(progress_tx)).await;
        let schema = tool.parameters_schema();
        if let Err(message) = validation::validate_and_coerce(&schema, &mut prepared.params) {
            return Self::immediate_tool_outcome(
                tool_call,
                &prepared.started_at,
                ToolResult::error_typed(
                    format!("Parameter validation failed: {}", message),
                    ToolErrorType::Validation,
                    true,
                    Some(format!("Fix the parameters and retry. Schema: {}", schema)),
                ),
            );
        }

        let mut result = match tokio::time::timeout(
            std::time::Duration::from_secs(120),
            tool.execute(prepared.params, &ctx),
        )
        .await
        {
            Ok(Ok(result)) => result,
            Ok(Err(err)) => {
                error!("Tool {} failed: {}", tool_call.name, err);
                ToolResult::error(format!("Tool execution failed: {}", err))
            }
            Err(_) => ToolResult::error_typed(
                format!("Tool execution timed out after 120s: {}", tool_call.name),
                ToolErrorType::Timeout,
                true,
                Some("Narrow the command scope or run a lighter probe first.".to_string()),
            ),
        };
        let elapsed = start_time.elapsed();
        debug!(
            tool = %tool_call.name,
            elapsed_ms = elapsed.as_millis() as u64,
            "Tool execution completed"
        );

        if result.is_error {
            let auto_hint = match result.error_type {
                Some(ToolErrorType::NotFound) => Some(
                    "Try using `glob` to find the correct path, or `grep` to search for the symbol by name."
                        .to_string(),
                ),
                Some(ToolErrorType::Validation) => Some(format!(
                    "Re-check parameter types and required fields. Schema: {}",
                    tool.parameters_schema()
                )),
                Some(ToolErrorType::Timeout) => Some(
                    "Reduce the scope of the operation (smaller file range, fewer results) and retry."
                        .to_string(),
                ),
                Some(ToolErrorType::Permission) => Some(
                    "This operation requires user confirmation. The user denied it — try an alternative approach."
                        .to_string(),
                ),
                _ => None,
            };

            if let Some(suggestion) = &result.suggestion {
                result
                    .content
                    .push_str(&format!("\n\nSuggestion: {}", suggestion));
            } else if let Some(hint) = auto_hint {
                result
                    .content
                    .push_str(&format!("\n\nSuggestion: {}", hint));
            }
        }

        let progress_updates = progress_counter.load(Ordering::Relaxed) as u32;
        let last_progress_message = last_progress_message
            .lock()
            .ok()
            .and_then(|slot| slot.clone());
        self.record_tool_progress_summary(
            &tool_call.name,
            progress_updates,
            last_progress_message.clone(),
        );

        ToolExecutionOutcome {
            tool_call: tool_call.clone(),
            result,
            started_at: prepared.started_at,
            duration_ms: elapsed.as_millis() as u64,
            progress_updates,
            last_progress_message,
            parallel_batch: None,
        }
    }
}

fn is_observer_tool(tool_name: &str) -> bool {
    [
        "ls",
        "glob",
        "grep",
        "git_status",
        "git_diff",
        "git_log",
        "project_map",
        "todo",
        "read_file",
    ]
    .contains(&tool_name)
}
