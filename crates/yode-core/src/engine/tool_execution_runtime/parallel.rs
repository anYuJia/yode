use super::*;

impl AgentEngine {
    /// Partition tool calls into (parallel, sequential) based on permission and read_only.
    pub(in crate::engine) fn partition_tool_calls(
        &self,
        tool_calls: &[ToolCall],
    ) -> (Vec<ToolCall>, Vec<ToolCall>) {
        let mut parallel = Vec::new();
        let mut sequential = Vec::new();

        if self.recovery_state != RecoveryState::Normal {
            return (parallel, tool_calls.to_vec());
        }

        for tc in tool_calls {
            let can_parallel = if let Some(tool) = self.tools.get(&tc.name) {
                let caps = tool.capabilities();
                caps.read_only
                    && matches!(self.permissions.check(&tc.name), PermissionAction::Allow)
            } else {
                false
            };

            if can_parallel {
                parallel.push(tc.clone());
            } else {
                sequential.push(tc.clone());
            }
        }

        (parallel, sequential)
    }

    /// Execute a batch of read-only, auto-allowed tool calls in parallel.
    pub(in crate::engine) async fn execute_tools_parallel(
        &mut self,
        tool_calls: &[ToolCall],
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) -> Vec<ToolExecutionOutcome> {
        use futures::future::join_all;

        let mut futures = Vec::new();
        let batch_id = self.register_parallel_batch(tool_calls.len());

        for tc in tool_calls {
            let tool = match self.tools.get(&tc.name) {
                Some(t) => t,
                None => continue,
            };

            let mut params: serde_json::Value = serde_json::from_str(&tc.arguments)
                .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
            let working_dir = self.current_runtime_working_dir().await;

            if let Some(blocked) = self
                .run_pre_tool_use_hook(&tc.name, &tc.arguments, &working_dir, &mut params)
                .await
            {
                let tc_clone = tc.clone();
                futures.push(Box::pin(async move {
                    ToolExecutionOutcome {
                        tool_call: tc_clone,
                        result: blocked,
                        started_at: Some(Self::now_timestamp()),
                        duration_ms: 0,
                        progress_updates: 0,
                        last_progress_message: None,
                        parallel_batch: Some(batch_id),
                    }
                })
                    as Pin<
                        Box<dyn std::future::Future<Output = ToolExecutionOutcome> + Send>,
                    >);
                continue;
            }

            let schema = tool.parameters_schema();
            if let Err(msg) = validation::validate_and_coerce(&schema, &mut params) {
                let tc_clone = tc.clone();
                let result = ToolResult::error_typed(
                    format!("Parameter validation failed: {}", msg),
                    ToolErrorType::Validation,
                    true,
                    Some(format!("Fix the parameters and retry. Schema: {}", schema)),
                );
                futures.push(Box::pin(async move {
                    ToolExecutionOutcome {
                        tool_call: tc_clone,
                        result,
                        started_at: Some(Self::now_timestamp()),
                        duration_ms: 0,
                        progress_updates: 0,
                        last_progress_message: None,
                        parallel_batch: Some(batch_id),
                    }
                })
                    as Pin<
                        Box<dyn std::future::Future<Output = ToolExecutionOutcome> + Send>,
                    >);
                continue;
            }
            let effective_arguments =
                serde_json::to_string(&params).unwrap_or_else(|_| tc.arguments.clone());

            let _ = event_tx.send(EngineEvent::ToolCallStart {
                id: tc.id.clone(),
                name: tc.name.clone(),
                arguments: effective_arguments,
            });

            info!(
                "Executing tool in parallel: {} (auto-allowed, read-only)",
                tc.name
            );

            let (p_tx, mut p_rx) = mpsc::unbounded_channel::<yode_tools::tool::ToolProgress>();
            let event_tx_inner = event_tx.clone();
            let tc_id = tc.id.clone();
            let tc_name = tc.name.clone();
            let progress_counter = Arc::new(AtomicU64::new(0));
            let progress_counter_inner = Arc::clone(&progress_counter);
            let last_progress_message = Arc::new(std::sync::Mutex::new(None::<String>));
            let last_progress_message_inner = Arc::clone(&last_progress_message);
            tokio::spawn(async move {
                while let Some(progress) = p_rx.recv().await {
                    progress_counter_inner.fetch_add(1, Ordering::Relaxed);
                    if let Ok(mut slot) = last_progress_message_inner.lock() {
                        *slot = Some(progress.message.clone());
                    }
                    let _ = event_tx_inner.send(EngineEvent::ToolProgress {
                        id: tc_id.clone(),
                        name: tc_name.clone(),
                        progress,
                    });
                }
            });

            let ctx = self.build_tool_context(Some(p_tx)).await;

            let tool_name = tc.name.clone();
            let tc_clone = tc.clone();
            let started_at = Some(Self::now_timestamp());

            futures.push(Box::pin(async move {
                let start = std::time::Instant::now();
                let timeout = std::time::Duration::from_secs(PARALLEL_TOOL_TIMEOUT_SECS);
                let result = match tokio::time::timeout(timeout, tool.execute(params, &ctx)).await {
                    Ok(Ok(r)) => r,
                    Ok(Err(e)) => {
                        error!("Tool {} failed: {}", tool_name, e);
                        ToolResult::error(format!("Tool execution failed: {}", e))
                    }
                    Err(_) => {
                        warn!(
                            "Tool {} timed out after {}s",
                            tool_name, PARALLEL_TOOL_TIMEOUT_SECS
                        );
                        ToolResult::error_typed(
                            format!(
                                "Tool {} timed out after {} seconds",
                                tool_name, PARALLEL_TOOL_TIMEOUT_SECS
                            ),
                            ToolErrorType::Timeout,
                            true,
                            Some("Try a smaller scope or more specific parameters.".to_string()),
                        )
                    }
                };
                debug!(
                    tool = %tool_name,
                    elapsed_ms = start.elapsed().as_millis() as u64,
                    "Parallel tool completed"
                );
                let progress_updates = progress_counter.load(Ordering::Relaxed) as u32;
                let last_progress_message = last_progress_message
                    .lock()
                    .ok()
                    .and_then(|slot| slot.clone());
                ToolExecutionOutcome {
                    tool_call: tc_clone,
                    result,
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    progress_updates,
                    last_progress_message,
                    parallel_batch: Some(batch_id),
                }
            }));
        }

        let outcomes = join_all(futures).await;
        for outcome in &outcomes {
            self.record_tool_progress_summary(
                &outcome.tool_call.name,
                outcome.progress_updates,
                outcome.last_progress_message.clone(),
            );
        }
        outcomes
    }
}
