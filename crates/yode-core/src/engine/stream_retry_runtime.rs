use super::*;

pub(super) enum StreamRetryAction {
    Continue,
    ReturnOk,
}

impl AgentEngine {
    pub(super) async fn retry_stream_after_error(
        &mut self,
        err: anyhow::Error,
        full_text: &mut String,
        full_reasoning: &mut String,
        tool_calls: &mut Vec<ToolCall>,
        final_response: &mut Option<ChatResponse>,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<StreamRetryAction> {
        let kind = classify_error(&err);
        let retry_reason = format!("{:?}", kind);
        *self
            .stream_retry_reason_histogram
            .entry(retry_reason)
            .or_insert(0) += 1;
        if kind == ErrorKind::Fatal {
            let _ = event_tx.send(EngineEvent::Error(format!(
                "Request failed after 1 attempt: {}",
                summarize_retry_error_message(&format!("{}", err))
            )));
            self.complete_tool_turn_artifact();
            let _ = event_tx.send(EngineEvent::Done);
            return Err(err).context("LLM chat request failed");
        }

        let max_attempts = max_retries_for(kind);
        let total_attempts = total_attempts_for(kind);
        let mut retry_succeeded = false;
        let mut final_error_summary = summarize_retry_error_message(&format!("{}", err));

        for attempt in 0..max_attempts {
            let delay = retry_delay(kind, attempt);
            let total_secs = delay.as_secs();

            for remaining in (0..=total_secs).rev() {
                let _ = event_tx.send(EngineEvent::Retrying {
                    error_message: final_error_summary.clone(),
                    attempt: attempt + 2,
                    max_attempts: total_attempts,
                    delay_secs: remaining,
                });
                if remaining > 0 {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    if let Some(token) = cancel_token {
                        if token.is_cancelled() {
                            self.complete_tool_turn_artifact();
                            let _ = event_tx.send(EngineEvent::Done);
                            return Ok(StreamRetryAction::ReturnOk);
                        }
                    }
                }
            }
            info!("Retrying stream (attempt {}/{})", attempt + 1, max_attempts);

            if let Some(token) = cancel_token {
                if token.is_cancelled() {
                    self.complete_tool_turn_artifact();
                    let _ = event_tx.send(EngineEvent::Done);
                    return Ok(StreamRetryAction::ReturnOk);
                }
            }

            let retry_request = self.build_chat_request();
            let (retry_tx, mut retry_rx) = mpsc::channel::<StreamEvent>(256);
            let retry_provider = self.provider.clone();
            let retry_handle = tokio::spawn(async move {
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(LLM_TIMEOUT_SECS),
                    retry_provider.chat_stream(retry_request, retry_tx),
                )
                .await;
                match result {
                    Ok(inner) => inner,
                    Err(_) => Err(anyhow::anyhow!("LLM 调用超时 ({}秒)", LLM_TIMEOUT_SECS)),
                }
            });

            let mut retry_cancelled = false;
            loop {
                if let Some(token) = cancel_token {
                    tokio::select! {
                        event = retry_rx.recv() => {
                            match event {
                                Some(ev) => {
                                    let is_done = matches!(ev, StreamEvent::Done(_));
                                    Self::process_stream_event(
                                        ev,
                                        full_text,
                                        full_reasoning,
                                        tool_calls,
                                        final_response,
                                        event_tx,
                                    );
                                    if is_done {
                                        break;
                                    }
                                }
                                None => break,
                            }
                        }
                        _ = token.cancelled() => {
                            retry_cancelled = true;
                            retry_handle.abort();
                            break;
                        }
                    }
                } else {
                    match retry_rx.recv().await {
                        Some(ev) => {
                            let is_done = matches!(ev, StreamEvent::Done(_));
                            Self::process_stream_event(
                                ev,
                                full_text,
                                full_reasoning,
                                tool_calls,
                                final_response,
                                event_tx,
                            );
                            if is_done {
                                break;
                            }
                        }
                        None => break,
                    }
                }
            }

            if retry_cancelled {
                if let Some(assistant_msg) =
                    self.build_partial_stream_assistant_message(full_text, full_reasoning)
                {
                    self.push_and_persist_assistant_message(&assistant_msg);
                }
                self.complete_tool_turn_artifact();
                let _ = event_tx.send(EngineEvent::Done);
                return Ok(StreamRetryAction::ReturnOk);
            }

            match retry_handle.await {
                Ok(Ok(())) => {
                    retry_succeeded = true;
                    break;
                }
                Ok(Err(e2)) => {
                    final_error_summary = summarize_retry_error_message(&format!("{}", e2));
                    warn!(
                        "Stream retry {}/{} failed: {}",
                        attempt + 1,
                        max_attempts,
                        e2
                    );
                }
                Err(e2) => {
                    final_error_summary =
                        summarize_retry_error_message(&format!("Stream task error: {}", e2));
                    warn!(
                        "Stream retry {}/{} panicked: {}",
                        attempt + 1,
                        max_attempts,
                        e2
                    );
                }
            }

            if !full_text.is_empty() || !tool_calls.is_empty() {
                retry_succeeded = true;
                break;
            }
        }

        if !retry_succeeded {
            let _ = event_tx.send(EngineEvent::Error(format!(
                "Request failed after {} attempts: {}",
                total_attempts, final_error_summary
            )));
            self.complete_tool_turn_artifact();
            let _ = event_tx.send(EngineEvent::Done);
            return Err(anyhow::anyhow!(
                "Request failed after {} attempts: {}",
                total_attempts, final_error_summary
            ))
            .context("LLM chat request failed");
        }

        Ok(StreamRetryAction::Continue)
    }
}
