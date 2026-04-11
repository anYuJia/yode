use super::*;

impl AgentEngine {
    /// Run one user turn: send user message, loop through tool calls until final text response.
    pub async fn run_turn(
        &mut self,
        user_input: &str,
        source: QuerySource,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
        mut confirm_rx: mpsc::UnboundedReceiver<ConfirmResponse>,
    ) -> Result<()> {
        self.current_query_source = source;
        self.rebuild_system_prompt();
        let _ = event_tx.send(EngineEvent::Thinking);
        self.append_turn_setup_context(user_input).await;
        self.record_turn_user_input(user_input);
        self.reset_turn_runtime_state();
        self.reset_non_streaming_error_state();

        loop {
            let _ = event_tx.send(EngineEvent::Thinking);

            let request = self.build_chat_request();
            let response = self.call_llm_with_retry(request).await?;

            self.record_response_usage(&response.usage, &event_tx);
            self.maybe_compact_context(response.usage.prompt_tokens, &event_tx)
                .await;

            debug!(
                "LLM response: text={:?}, tool_calls={}",
                response.message.content.as_deref().unwrap_or(""),
                response.message.tool_calls.len()
            );

            let mut assistant_msg = response.message.clone();

            if response.stop_reason == Some(yode_llm::types::StopReason::MaxTokens) {
                let warning = "\n\n[WARNING: Response truncated due to max_tokens limit. Consider increasing effort level if more detail is needed.]";
                if let Some(content) = &mut assistant_msg.content {
                    content.push_str(warning);
                } else {
                    assistant_msg.content = Some(warning.to_string());
                }
                warn!("LLM response truncated due to max_tokens");
            } else if response.stop_reason == Some(yode_llm::types::StopReason::StopSequence)
                || matches!(
                    response.stop_reason,
                    Some(yode_llm::types::StopReason::Other(_))
                )
            {
                if let Some(ref content) = assistant_msg.content {
                    if content.contains("[tool_") || content.contains("<tool_") {
                        warn!("LLM response stopped via stop sequence or other reason but contains incomplete tool tags. Reason: {:?}", response.stop_reason);
                    }
                }
            }

            if let Some(ref content) = assistant_msg.content {
                if content.contains("[tool_use") || content.contains("[DUMMY_TOOL") {
                    assistant_msg.content = Some(self.clean_assistant_response(content));
                }
            }

            if assistant_msg.tool_calls.is_empty() {
                if let Some(content) = assistant_msg.content.clone() {
                    let recovered = self.recover_leaked_tool_calls(&content);
                    if !recovered.is_empty() {
                        info!(
                            "Recovered {} leaked tool calls from text response (non-streaming).",
                            recovered.len()
                        );
                        assistant_msg.tool_calls = recovered;
                        self.violation_retries = 0;
                    } else if self.is_protocol_violation(&content) {
                        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
                        let bucket = self
                            .error_buckets
                            .entry(ToolErrorType::Protocol)
                            .or_insert(0);
                        *bucket += 1;
                    }
                }
            } else {
                self.violation_retries = 0;
            }

            assistant_msg.normalize_in_place();
            self.push_and_persist_assistant_message(&assistant_msg);

            if !assistant_msg.tool_calls.is_empty() {
                debug!(
                    "Tool batch incoming: total={}, consecutive_failures={}, recent_calls={}",
                    assistant_msg.tool_calls.len(),
                    self.consecutive_failures,
                    self.recent_tool_calls.len()
                );
                let (parallel, sequential) = self.partition_tool_calls(&assistant_msg.tool_calls);

                let parallel_results = if !parallel.is_empty() {
                    info!("Executing {} tools in parallel", parallel.len());
                    self.execute_tools_parallel(&parallel, &event_tx).await
                } else {
                    vec![]
                };

                for outcome in parallel_results {
                    self.record_completed_tool_outcome(outcome, &event_tx).await;
                }

                for tool_call in &sequential {
                    let outcome = self
                        .handle_tool_call(tool_call, &event_tx, &mut confirm_rx, None)
                        .await?;
                    self.record_completed_tool_outcome(outcome, &event_tx).await;
                }

                continue;
            }

            if let Some(text) = &response.message.content {
                if self.consecutive_failures >= 2 && self.files_read.is_empty() {
                    let guarded = format!(
                        "{}\n\n[EVIDENCE GATE: Multiple failures occurred and no successful file reads were recorded in this turn. Summarize verified facts only and ask for directory/path confirmation before concluding.]",
                        text
                    );
                    let _ = event_tx.send(EngineEvent::TextComplete(guarded));
                } else {
                    let _ = event_tx.send(EngineEvent::TextComplete(text.clone()));
                }
            }

            self.maybe_refresh_live_session_memory(Some(&event_tx));
            self.complete_tool_turn_artifact();
            self.complete_turn_runtime_artifact(response.stop_reason.as_ref());
            let _ = event_tx.send(EngineEvent::TurnComplete(response));
            let _ = event_tx.send(EngineEvent::Done);
            break;
        }

        Ok(())
    }
}
