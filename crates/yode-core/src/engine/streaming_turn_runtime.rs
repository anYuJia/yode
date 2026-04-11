use super::*;
use super::stream_retry_runtime::StreamRetryAction;

impl AgentEngine {
    /// Run one user turn with streaming LLM output.
    /// Accepts an optional CancellationToken for cooperative cancellation.
    pub async fn run_turn_streaming(
        &mut self,
        user_input: &str,
        source: QuerySource,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
        mut confirm_rx: mpsc::UnboundedReceiver<ConfirmResponse>,
        cancel_token: Option<CancellationToken>,
    ) -> Result<()> {
        self.current_query_source = source;
        self.rebuild_system_prompt();
        self.append_turn_setup_context(user_input).await;
        self.record_turn_user_input(user_input);
        self.reset_turn_runtime_state();

        loop {
            if let Some(ref token) = cancel_token {
                if token.is_cancelled() {
                    self.complete_tool_turn_artifact();
                    let _ = event_tx.send(EngineEvent::Done);
                    return Ok(());
                }
            }

            let _ = event_tx.send(EngineEvent::Thinking);
            let request = self.build_chat_request();

            let (stream_tx, mut stream_rx) = mpsc::channel::<StreamEvent>(256);
            let turn_start = std::time::Instant::now();
            let hard_turn_timeout = std::time::Duration::from_secs(600);

            let provider = self.provider.clone();
            let stream_handle = tokio::spawn(async move {
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(LLM_TIMEOUT_SECS.max(600)),
                    provider.chat_stream(request, stream_tx),
                )
                .await;
                match result {
                    Ok(inner) => inner,
                    Err(_) => Err(anyhow::anyhow!(
                        "LLM 调用超时 ({}秒)",
                        LLM_TIMEOUT_SECS.max(600)
                    )),
                }
            });

            let mut full_text = String::new();
            let mut full_reasoning = String::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();
            let mut final_response: Option<ChatResponse> = None;
            let mut cancelled = false;
            let mut stalled = false;
            let mut last_progress_at = std::time::Instant::now();
            let stall_timeout = std::time::Duration::from_secs(120);

            loop {
                if turn_start.elapsed() > hard_turn_timeout {
                    warn!(
                        "Streaming turn timed out after {:?}; forcing completion",
                        hard_turn_timeout
                    );
                    stream_handle.abort();
                    stalled = true;
                    break;
                }

                if last_progress_at.elapsed() > stall_timeout {
                    warn!(
                        "Streaming stalled for {:?} without progress; forcing completion",
                        stall_timeout
                    );
                    stream_handle.abort();
                    stalled = true;
                    break;
                }

                if let Some(ref token) = cancel_token {
                    tokio::select! {
                        event = stream_rx.recv() => {
                            match event {
                                Some(ev) => {
                                    last_progress_at = std::time::Instant::now();
                                    let is_done = matches!(ev, StreamEvent::Done(_));
                                    Self::process_stream_event(ev, &mut full_text, &mut full_reasoning, &mut tool_calls, &mut final_response, &event_tx);
                                    if is_done { break; }
                                }
                                None => break,
                            }
                        }
                        _ = token.cancelled() => {
                            cancelled = true;
                            stream_handle.abort();
                            break;
                        }
                        _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {
                            let _ = event_tx.send(EngineEvent::Thinking);
                        }
                    }
                } else {
                    match tokio::time::timeout(std::time::Duration::from_secs(2), stream_rx.recv())
                        .await
                    {
                        Ok(Some(ev)) => {
                            last_progress_at = std::time::Instant::now();
                            let is_done = matches!(ev, StreamEvent::Done(_));
                            Self::process_stream_event(
                                ev,
                                &mut full_text,
                                &mut full_reasoning,
                                &mut tool_calls,
                                &mut final_response,
                                &event_tx,
                            );
                            if is_done {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(_) => {
                            let _ = event_tx.send(EngineEvent::Thinking);
                        }
                    }
                }
            }

            if cancelled || stalled {
                if let Some(assistant_msg) =
                    self.build_partial_stream_assistant_message(&full_text, &full_reasoning)
                {
                    self.push_and_persist_assistant_message(&assistant_msg);
                }
                if stalled {
                    let _ = event_tx.send(EngineEvent::TextComplete(
                        "[Watchdog] Streaming stalled; forcing graceful stop. Please retry with narrower scope.".to_string(),
                    ));
                }
                self.complete_tool_turn_artifact();
                let _ = event_tx.send(EngineEvent::Done);
                return Ok(());
            }

            if !cancelled {
                let stream_result = stream_handle.await;
                let stream_err = match stream_result {
                    Ok(Ok(())) => None,
                    Ok(Err(e)) => {
                        warn!("Stream failed: {}", e);
                        Some(e)
                    }
                    Err(e) => {
                        warn!("Stream task panicked: {}", e);
                        Some(anyhow::anyhow!("Stream task error: {}", e))
                    }
                };

                if let Some(err) = stream_err {
                    if full_text.is_empty() && tool_calls.is_empty() {
                        match self
                            .retry_stream_after_error(
                                err,
                                &mut full_text,
                                &mut full_reasoning,
                                &mut tool_calls,
                                &mut final_response,
                                &event_tx,
                                cancel_token.as_ref(),
                            )
                            .await?
                        {
                            StreamRetryAction::Continue => {}
                            StreamRetryAction::ReturnOk => return Ok(()),
                        }
                    }
                }
            }

            if let Some(ref resp) = final_response {
                self.record_response_usage(&resp.usage, &event_tx);
                self.maybe_compact_context(resp.usage.prompt_tokens, &event_tx)
                    .await;
            }

            let mut assistant_msg = Message {
                role: Role::Assistant,
                content: if full_text.is_empty() {
                    None
                } else {
                    Some(full_text.clone())
                },
                reasoning: if full_reasoning.is_empty() {
                    None
                } else {
                    Some(full_reasoning.clone())
                },
                content_blocks: Vec::new(),
                tool_calls: tool_calls.clone(),
                tool_call_id: None,
                images: Vec::new(),
            };

            if let Some(ref resp) = final_response {
                if resp.stop_reason == Some(yode_llm::types::StopReason::MaxTokens) {
                    let warning = "\n\n[WARNING: Response truncated due to max_tokens limit. Consider increasing effort level if more detail is needed.]";
                    if let Some(content) = &mut assistant_msg.content {
                        content.push_str(warning);
                        full_text = content.clone();
                    } else {
                        assistant_msg.content = Some(warning.to_string());
                        full_text = warning.to_string();
                    }
                    warn!("LLM streaming response truncated due to max_tokens");
                } else if (resp.stop_reason == Some(yode_llm::types::StopReason::StopSequence)
                    || matches!(
                        resp.stop_reason,
                        Some(yode_llm::types::StopReason::Other(_))
                    ))
                    && (full_text.contains("[tool_") || full_text.contains("<tool_"))
                {
                    warn!("LLM streaming response stopped via stop sequence or other reason but contains incomplete tool tags. Reason: {:?}", resp.stop_reason);
                }
            }

            if assistant_msg.tool_calls.is_empty()
                && !full_text.is_empty()
                && self.is_protocol_violation(&full_text)
            {
                let recovered = self.recover_leaked_tool_calls(&full_text);
                if !recovered.is_empty() {
                    info!("Recovered {} leaked tool calls from text response. Proceeding with execution.", recovered.len());
                    assistant_msg.tool_calls = recovered;
                    tool_calls = assistant_msg.tool_calls.clone();
                    self.violation_retries = 0;
                } else if self.violation_retries < 2 {
                    self.violation_retries += 1;
                    warn!("Protocol violation detected (attempt {}). Retrying with strict constraints...", self.violation_retries);
                    let _ = event_tx.send(EngineEvent::Thinking);

                    self.messages.push(Message::user(
                        "STRICT PROTOCOL VIOLATION: You outputted internal tool tags ([tool_use], [DUMMY_TOOL], etc.) in your text response. \
                         This is forbidden. Please respond again using ONLY natural language. Do NOT use tool tags or JSON in this response."
                    ));
                    continue;
                } else {
                    let err_msg = "Critical protocol failure: Model repeatedly outputted internal tool tags in text field. Aborting to prevent loop.";
                    error!("{}", err_msg);
                    let _ = event_tx.send(EngineEvent::Error(err_msg.to_string()));
                    let _ = event_tx.send(EngineEvent::Done);
                    return Ok(());
                }
            } else {
                self.violation_retries = 0;
            }

            if let Some(ref text) = assistant_msg.content {
                if self.is_protocol_violation(text) {
                    assistant_msg.content = Some(self.clean_assistant_response(text));
                    full_text = assistant_msg.content.as_ref().unwrap().clone();
                }
            }

            if !full_reasoning.is_empty() {
                assistant_msg
                    .content_blocks
                    .push(yode_llm::types::ContentBlock::Thinking {
                        thinking: full_reasoning.clone(),
                        signature: None,
                    });
            }
            if !full_text.is_empty() {
                assistant_msg
                    .content_blocks
                    .push(yode_llm::types::ContentBlock::Text {
                        text: full_text.clone(),
                    });
            }

            assistant_msg.normalize_in_place();
            self.push_and_persist_assistant_message(&assistant_msg);

            if !tool_calls.is_empty() {
                let (parallel, sequential) = self.partition_tool_calls(&tool_calls);

                if !parallel.is_empty() {
                    if let Some(ref token) = cancel_token {
                        if token.is_cancelled() {
                            self.complete_tool_turn_artifact();
                            let _ = event_tx.send(EngineEvent::Done);
                            return Ok(());
                        }
                    }

                    info!("Executing {} tools in parallel (streaming)", parallel.len());
                    let parallel_results = self.execute_tools_parallel(&parallel, &event_tx).await;

                    for outcome in parallel_results {
                        self.record_completed_tool_outcome(outcome, &event_tx).await;
                    }
                }

                for tool_call in &sequential {
                    if let Some(ref token) = cancel_token {
                        if token.is_cancelled() {
                            self.complete_tool_turn_artifact();
                            let _ = event_tx.send(EngineEvent::Done);
                            return Ok(());
                        }
                    }

                    let outcome = self
                        .handle_tool_call(
                            tool_call,
                            &event_tx,
                            &mut confirm_rx,
                            cancel_token.as_ref(),
                        )
                        .await?;
                    self.record_completed_tool_outcome(outcome, &event_tx).await;
                }
                continue;
            }

            if let Some(mut resp) = final_response {
                if resp.message.tool_calls.is_empty() {
                    if let Some(content) = resp.message.content.clone() {
                        if content.len() > 3000 {
                            let trimmed: String = content.chars().take(1800).collect();
                            resp.message.content = Some(format!(
                                "{}\n\n[Output truncated by runtime guard to keep response responsive. Ask to continue if you want more details.]",
                                trimmed
                            ));
                        }
                    }
                }
                if resp.message.content.is_none() && !full_text.is_empty() {
                    resp.message.content = Some(full_text.clone());
                }
                if resp.message.reasoning.is_none() && !full_reasoning.is_empty() {
                    resp.message.reasoning = Some(full_reasoning.clone());
                }

                let content_for_analysis = resp.message.content.clone();
                if let Some(content) = content_for_analysis {
                    if content.contains("[tool_use") || content.contains("[DUMMY_TOOL") {
                        resp.message.content = Some(self.clean_assistant_response(&content));
                    }

                    if resp.message.tool_calls.is_empty() && content.contains("[tool_use") {
                        warn!("Detected leaked tool-use text; skipping text-recovery to avoid invalid tool schema propagation");
                    }
                }

                self.push_and_persist_assistant_message(&resp.message);

                if resp.message.tool_calls.is_empty() {
                    debug!("Streaming turn complete with no tool calls; finishing turn.");
                    self.maybe_refresh_live_session_memory(Some(&event_tx));
                    self.complete_tool_turn_artifact();
                    let _ = event_tx.send(EngineEvent::TurnComplete(resp));
                    let _ = event_tx.send(EngineEvent::Done);
                    break;
                } else {
                    debug!(
                        "Streaming turn produced {} tool calls; continuing loop.",
                        resp.message.tool_calls.len()
                    );
                    let _ = event_tx.send(EngineEvent::TurnComplete(resp));
                }
            } else {
                self.complete_tool_turn_artifact();
                let _ = event_tx.send(EngineEvent::Done);
                break;
            }
        }

        Ok(())
    }
}
