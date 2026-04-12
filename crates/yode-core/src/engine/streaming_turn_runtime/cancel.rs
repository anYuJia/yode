use super::*;

impl AgentEngine {
    pub(in crate::engine) fn turn_cancelled(
        &mut self,
        cancel_token: Option<&CancellationToken>,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) -> bool {
        if cancel_token.is_some_and(|token| token.is_cancelled()) {
            self.complete_tool_turn_artifact();
            let _ = event_tx.send(EngineEvent::Done);
            return true;
        }
        false
    }

    pub(in crate::engine) async fn handle_interrupted_stream(
        &mut self,
        cancelled: bool,
        stalled: bool,
        buffers: &StreamTurnBuffers,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) -> bool {
        if !cancelled && !stalled {
            return false;
        }

        if let Some(assistant_msg) =
            self.build_partial_stream_assistant_message(&buffers.full_text, &buffers.full_reasoning)
        {
            self.push_and_persist_assistant_message(&assistant_msg);
        }
        if stalled {
            let _ = event_tx.send(EngineEvent::TextComplete(
                "[Watchdog] Streaming stalled; forcing graceful stop. Please retry with narrower scope."
                    .to_string(),
            ));
        }
        self.complete_tool_turn_artifact();
        let _ = event_tx.send(EngineEvent::Done);
        true
    }
}
