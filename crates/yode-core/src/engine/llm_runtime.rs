use super::*;
use super::retry::summarize_retry_error;

impl AgentEngine {
    /// Process a single stream event.
    pub(super) fn process_stream_event(
        event: StreamEvent,
        full_text: &mut String,
        full_reasoning: &mut String,
        tool_calls: &mut Vec<ToolCall>,
        final_response: &mut Option<ChatResponse>,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) {
        match event {
            StreamEvent::TextDelta(delta) => {
                full_text.push_str(&delta);
                let _ = event_tx.send(EngineEvent::TextDelta(delta));
            }
            StreamEvent::UsageUpdate(usage) => {
                let _ = event_tx.send(EngineEvent::UsageUpdate(usage));
            }
            StreamEvent::ReasoningDelta(delta) => {
                full_reasoning.push_str(&delta);
                let _ = event_tx.send(EngineEvent::ReasoningDelta(delta));
            }
            StreamEvent::ToolCallStart { id, name } => {
                tool_calls.push(ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: String::new(),
                });
                let _ = event_tx.send(EngineEvent::ToolCallStart {
                    id,
                    name,
                    arguments: String::new(),
                });
            }
            StreamEvent::ToolCallDelta { id, arguments } => {
                if let Some(tc) = tool_calls.iter_mut().find(|t| t.id == id) {
                    tc.arguments.push_str(&arguments);
                }
            }
            StreamEvent::ToolCallEnd { id: _ } => {}
            StreamEvent::Done(resp) => {
                if !full_reasoning.is_empty() {
                    let _ = event_tx.send(EngineEvent::ReasoningComplete(full_reasoning.clone()));
                }
                *final_response = Some(resp);
            }
            StreamEvent::Error(_e) => {}
        }
    }

    /// Call LLM with retry logic (non-streaming). Classifies errors and uses appropriate backoff.
    pub(super) async fn call_llm_with_retry(&self, request: ChatRequest) -> Result<ChatResponse> {
        self.call_llm_with_retry_notify(request, None).await
    }

    /// Call LLM with retry logic, optionally notifying the UI about retries.
    pub(super) async fn call_llm_with_retry_notify(
        &self,
        request: ChatRequest,
        event_tx: Option<&mpsc::UnboundedSender<EngineEvent>>,
    ) -> Result<ChatResponse> {
        let mut last_err = None;
        let mut attempt: u32 = 0;
        let mut max_attempts = max_retries_for(ErrorKind::Transient);

        loop {
            if attempt > 0 && attempt <= max_attempts {
                let kind = last_err
                    .as_ref()
                    .map(classify_error)
                    .unwrap_or(ErrorKind::Transient);
                let total_attempts = total_attempts_for(kind);
                let delay = retry_delay(kind, attempt - 1);
                let total_secs = delay.as_secs();
                if let Some(tx) = event_tx {
                    for remaining in (0..=total_secs).rev() {
                        let _ = tx.send(EngineEvent::Retrying {
                            error_message: summarize_retry_error(last_err.as_ref().unwrap()),
                            attempt: attempt + 1,
                            max_attempts: total_attempts,
                            delay_secs: remaining,
                        });
                        if remaining > 0 {
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        }
                    }
                } else {
                    tokio::time::sleep(delay).await;
                }
                info!("Retrying LLM call (attempt {}/{})", attempt, max_attempts);
            }

            if attempt > max_attempts {
                break;
            }

            let result = tokio::time::timeout(
                std::time::Duration::from_secs(LLM_TIMEOUT_SECS),
                self.provider.chat(request.clone()),
            )
            .await;

            match result {
                Ok(Ok(response)) => return Ok(response),
                Ok(Err(e)) => {
                    let kind = classify_error(&e);
                    if kind == ErrorKind::Fatal {
                        return Err(e).context("LLM chat request failed");
                    }
                    max_attempts = max_retries_for(kind);
                    warn!(
                        "LLM call failed (attempt {}/{}): {}",
                        attempt + 1,
                        max_attempts,
                        e
                    );
                    last_err = Some(e);
                }
                Err(_) => {
                    let err = anyhow::anyhow!("LLM 调用超时 ({}秒)", LLM_TIMEOUT_SECS);
                    max_attempts = max_retries_for(ErrorKind::Transient);
                    warn!(
                        "LLM call timed out (attempt {}/{})",
                        attempt + 1,
                        max_attempts
                    );
                    last_err = Some(err);
                }
            }
            attempt += 1;
        }

        let final_err = last_err.unwrap_or_else(|| anyhow::anyhow!("LLM call failed"));
        let kind = classify_error(&final_err);
        let total_attempts = total_attempts_for(kind);
        Err(anyhow::anyhow!(
            "Request failed after {} attempts: {}",
            total_attempts,
            summarize_retry_error(&final_err)
        ))
        .context("LLM chat request failed")
    }
}
