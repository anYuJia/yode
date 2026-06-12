use super::retry::summarize_retry_error;
use super::*;

impl AgentEngine {
    const ACTION_NARRATIVE_OPEN: &'static str = "<action_narrative>";
    const ACTION_NARRATIVE_CLOSE: &'static str = "</action_narrative>";

    fn sanitize_action_narrative(text: &str) -> Option<String> {
        let clean = text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
        if clean.is_empty() || clean.chars().count() > 120 {
            return None;
        }
        Some(clean)
    }

    fn trailing_action_tag_prefix_len(text: &str) -> usize {
        let tag = Self::ACTION_NARRATIVE_OPEN;
        let max_len = text.len().min(tag.len().saturating_sub(1));
        for len in (1..=max_len).rev() {
            if text.is_char_boundary(text.len() - len) && tag.starts_with(&text[text.len() - len..])
            {
                return len;
            }
        }
        0
    }

    fn strip_action_narratives(text: &str) -> (String, Vec<String>) {
        let mut rest = text;
        let mut clean = String::new();
        let mut narratives = Vec::new();

        loop {
            let Some(start) = rest.find(Self::ACTION_NARRATIVE_OPEN) else {
                clean.push_str(rest);
                break;
            };
            clean.push_str(&rest[..start]);
            let after_open = &rest[start + Self::ACTION_NARRATIVE_OPEN.len()..];
            let Some(end) = after_open.find(Self::ACTION_NARRATIVE_CLOSE) else {
                break;
            };
            if let Some(narrative) = Self::sanitize_action_narrative(&after_open[..end]) {
                narratives.push(narrative);
            }
            rest = &after_open[end + Self::ACTION_NARRATIVE_CLOSE.len()..];
        }

        (clean, narratives)
    }

    fn process_public_text_delta(
        delta: String,
        pending_text: &mut String,
        full_text: &mut String,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) {
        pending_text.push_str(&delta);

        loop {
            if let Some(start) = pending_text.find(Self::ACTION_NARRATIVE_OPEN) {
                let before = pending_text[..start].to_string();
                if !before.is_empty() {
                    full_text.push_str(&before);
                    let _ = event_tx.send(EngineEvent::TextDelta(before));
                }

                let after_open_start = start + Self::ACTION_NARRATIVE_OPEN.len();
                let Some(end) = pending_text[after_open_start..].find(Self::ACTION_NARRATIVE_CLOSE)
                else {
                    let kept = pending_text[start..].to_string();
                    pending_text.clear();
                    pending_text.push_str(&kept);
                    break;
                };

                let body_end = after_open_start + end;
                if let Some(narrative) =
                    Self::sanitize_action_narrative(&pending_text[after_open_start..body_end])
                {
                    let _ = event_tx.send(EngineEvent::ActionNarrative(narrative));
                }
                let after_close = body_end + Self::ACTION_NARRATIVE_CLOSE.len();
                let rest = pending_text[after_close..].to_string();
                pending_text.clear();
                pending_text.push_str(&rest);
                continue;
            }

            let keep_len = Self::trailing_action_tag_prefix_len(pending_text);
            let emit_len = pending_text.len().saturating_sub(keep_len);
            if emit_len > 0 {
                let emit = pending_text[..emit_len].to_string();
                full_text.push_str(&emit);
                let _ = event_tx.send(EngineEvent::TextDelta(emit));
                let rest = pending_text[emit_len..].to_string();
                pending_text.clear();
                pending_text.push_str(&rest);
            }
            break;
        }
    }

    fn flush_public_text_pending(
        pending_text: &mut String,
        full_text: &mut String,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) {
        if pending_text.is_empty() {
            return;
        }
        let (clean, narratives) = Self::strip_action_narratives(pending_text);
        for narrative in narratives {
            let _ = event_tx.send(EngineEvent::ActionNarrative(narrative));
        }
        if !clean.is_empty() {
            full_text.push_str(&clean);
            let _ = event_tx.send(EngineEvent::TextDelta(clean));
        }
        pending_text.clear();
    }

    pub(super) fn sanitize_action_narratives_in_text(text: &str) -> String {
        Self::strip_action_narratives(text).0
    }

    pub(super) fn split_action_narratives_from_text(text: &str) -> (String, Vec<String>) {
        Self::strip_action_narratives(text)
    }

    /// Process a single stream event.
    pub(super) fn process_stream_event(
        event: StreamEvent,
        full_text: &mut String,
        pending_text: &mut String,
        full_reasoning: &mut String,
        tool_calls: &mut Vec<ToolCall>,
        final_response: &mut Option<ChatResponse>,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) {
        match event {
            StreamEvent::TextDelta(delta) => {
                Self::process_public_text_delta(delta, pending_text, full_text, event_tx);
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
            }
            StreamEvent::ToolCallDelta { id, arguments } => {
                if let Some(tc) = tool_calls.iter_mut().find(|t| t.id == id) {
                    tc.arguments.push_str(&arguments);
                }
            }
            StreamEvent::ToolCallEnd { id: _ } => {}
            StreamEvent::Done(resp) => {
                Self::flush_public_text_pending(pending_text, full_text, event_tx);
                if !full_reasoning.is_empty() {
                    let _ = event_tx.send(EngineEvent::ReasoningComplete(full_reasoning.clone()));
                }
                let mut response = resp;
                if let Some(content) = response.message.content.as_ref() {
                    let clean = Self::sanitize_action_narratives_in_text(content);
                    if clean != *content {
                        response.message.content =
                            if clean.is_empty() { None } else { Some(clean) };
                        response.message.normalize_in_place();
                    }
                }
                *final_response = Some(response);
            }
            StreamEvent::Error(_e) => {}
        }
    }

    /// Call LLM with retry logic (non-streaming). Classifies errors and uses appropriate backoff.
    pub(super) async fn call_llm_with_retry(
        &mut self,
        request: ChatRequest,
    ) -> Result<ChatResponse> {
        self.call_llm_with_retry_notify(request, None).await
    }

    /// Call LLM with retry logic, optionally notifying the UI about retries.
    pub(super) async fn call_llm_with_retry_notify(
        &mut self,
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
                    let retry_message = last_err
                        .as_ref()
                        .map(summarize_retry_error)
                        .unwrap_or_else(|| "LLM request failed before retry".to_string());
                    for remaining in (0..=total_secs).rev() {
                        let _ = tx.send(EngineEvent::Retrying {
                            error_message: retry_message.clone(),
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

            let api_start = std::time::Instant::now();
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(LLM_TIMEOUT_SECS),
                self.provider.chat(request.clone()),
            )
            .await;
            self.cost_tracker.record_api_duration(api_start.elapsed());

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
                    let err = anyhow::anyhow!(EngineError::LlmTimeout {
                        timeout_secs: LLM_TIMEOUT_SECS,
                    });
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
        Err(anyhow::anyhow!(EngineError::LlmRetryExhausted {
            attempts: total_attempts,
            message: summarize_retry_error(&final_err),
        }))
        .context("LLM chat request failed")
    }
}
