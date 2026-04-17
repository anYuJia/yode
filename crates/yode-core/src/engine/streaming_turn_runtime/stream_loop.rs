use super::*;
use crate::engine::retry::summarize_retry_error;

pub(super) struct StreamLoopState {
    pub(super) cancelled: bool,
    pub(super) stalled: bool,
}

impl AgentEngine {
    pub(super) async fn run_stream_receive_loop(
        &mut self,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
        stream_rx: &mut mpsc::Receiver<StreamEvent>,
        stream_handle: &tokio::task::JoinHandle<Result<()>>,
        turn_start: std::time::Instant,
        cancel_token: Option<&CancellationToken>,
        buffers: &mut StreamTurnBuffers,
    ) -> StreamLoopState {
        let hard_turn_timeout = std::time::Duration::from_secs(600);
        let stall_timeout = std::time::Duration::from_secs(120);
        let mut cancelled = false;
        let mut stalled = false;
        let mut last_progress_at = std::time::Instant::now();

        loop {
            if turn_start.elapsed() > hard_turn_timeout {
                self.last_stream_watchdog_stage = Some("receive_loop:hard_timeout".to_string());
                warn!(
                    "Streaming turn timed out after {:?}; forcing completion",
                    hard_turn_timeout
                );
                stream_handle.abort();
                stalled = true;
                break;
            }

            if last_progress_at.elapsed() > stall_timeout {
                self.last_stream_watchdog_stage = Some("receive_loop:stall_timeout".to_string());
                warn!(
                    "Streaming stalled for {:?} without progress; forcing completion",
                    stall_timeout
                );
                stream_handle.abort();
                stalled = true;
                break;
            }

            if let Some(token) = cancel_token {
                tokio::select! {
                    event = stream_rx.recv() => {
                        match event {
                            Some(stream_event) => {
                                last_progress_at = std::time::Instant::now();
                                let is_done = matches!(stream_event, StreamEvent::Done(_));
                                Self::process_stream_event(
                                    stream_event,
                                    &mut buffers.full_text,
                                    &mut buffers.full_reasoning,
                                    &mut buffers.tool_calls,
                                    &mut buffers.final_response,
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
                    Ok(Some(stream_event)) => {
                        last_progress_at = std::time::Instant::now();
                        let is_done = matches!(stream_event, StreamEvent::Done(_));
                        Self::process_stream_event(
                            stream_event,
                            &mut buffers.full_text,
                            &mut buffers.full_reasoning,
                            &mut buffers.tool_calls,
                            &mut buffers.final_response,
                            event_tx,
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

        StreamLoopState { cancelled, stalled }
    }
    pub(super) async fn handle_stream_task_completion(
        &mut self,
        stream_handle: tokio::task::JoinHandle<Result<()>>,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
        cancel_token: Option<&CancellationToken>,
        buffers: &mut StreamTurnBuffers,
    ) -> Result<Option<StreamRetryAction>> {
        let stream_result = stream_handle.await;
        let stream_err = match stream_result {
            Ok(Ok(())) => None,
            Ok(Err(error)) => {
                warn!("Stream failed: {}", error);
                Some(error)
            }
            Err(error) => {
                warn!("Stream task panicked: {}", error);
                Some(anyhow::anyhow!("Stream task error: {}", error))
            }
        };

        if let Some(error) = stream_err {
            if buffers.full_text.is_empty() && buffers.tool_calls.is_empty() {
                let action = self
                    .retry_stream_after_error(
                        error,
                        &mut buffers.full_text,
                        &mut buffers.full_reasoning,
                        &mut buffers.tool_calls,
                        &mut buffers.final_response,
                        event_tx,
                        cancel_token,
                    )
                    .await?;
                return Ok(Some(action));
            } else {
                let _ = event_tx.send(EngineEvent::Error(format!(
                    "Stream ended early: {}",
                    summarize_retry_error(&error)
                )));
            }
        }

        Ok(None)
    }
}
