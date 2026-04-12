mod cancel;
mod finalization;
mod protocol;
mod stream_loop;
mod tool_calls;

use super::stream_retry_runtime::StreamRetryAction;
use super::*;

#[derive(Default)]
pub(super) struct StreamTurnBuffers {
    pub(super) full_text: String,
    pub(super) full_reasoning: String,
    pub(super) tool_calls: Vec<ToolCall>,
    pub(super) final_response: Option<ChatResponse>,
}

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
            if self.turn_cancelled(cancel_token.as_ref(), &event_tx) {
                return Ok(());
            }

            let request = self.begin_stream_turn(&event_tx);
            let provider = self.provider.clone();
            let (stream_tx, mut stream_rx) = mpsc::channel::<StreamEvent>(256);
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

            let turn_start = std::time::Instant::now();
            let mut buffers = StreamTurnBuffers::default();
            let stream_state = self
                .run_stream_receive_loop(
                    &event_tx,
                    &mut stream_rx,
                    &stream_handle,
                    turn_start,
                    cancel_token.as_ref(),
                    &mut buffers,
                )
                .await;

            if self
                .handle_interrupted_stream(stream_state.cancelled, stream_state.stalled, &buffers, &event_tx)
                .await
            {
                return Ok(());
            }

            if !stream_state.cancelled {
                if let Some(action) = self
                    .handle_stream_task_completion(
                        stream_handle,
                        &event_tx,
                        cancel_token.as_ref(),
                        &mut buffers,
                    )
                    .await?
                {
                    match action {
                        StreamRetryAction::Continue => {}
                        StreamRetryAction::ReturnOk => return Ok(()),
                    }
                }
            }

            if let Some(ref response) = buffers.final_response {
                self.record_response_usage(&response.usage, &event_tx);
                self.maybe_compact_context(response.usage.prompt_tokens, &event_tx)
                    .await;
            }

            match self
                .finalize_stream_turn(buffers, &event_tx, &mut confirm_rx, cancel_token.as_ref())
                .await?
            {
                finalization::StreamFinalizeAction::Continue => continue,
                finalization::StreamFinalizeAction::ReturnOk => return Ok(()),
                finalization::StreamFinalizeAction::Break => break,
            }
        }

        Ok(())
    }

    fn begin_stream_turn(&self, event_tx: &mpsc::UnboundedSender<EngineEvent>) -> ChatRequest {
        let _ = event_tx.send(EngineEvent::Thinking);
        self.build_chat_request()
    }
}
