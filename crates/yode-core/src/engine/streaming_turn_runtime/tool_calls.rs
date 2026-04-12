use super::*;

impl AgentEngine {
    pub(in crate::engine) async fn execute_stream_tool_calls(
        &mut self,
        tool_calls: &[ToolCall],
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
        confirm_rx: &mut mpsc::UnboundedReceiver<ConfirmResponse>,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<()> {
        let (parallel, sequential) = self.partition_tool_calls(tool_calls);

        if !parallel.is_empty() {
            if let Some(token) = cancel_token {
                if token.is_cancelled() {
                    self.complete_tool_turn_artifact();
                    let _ = event_tx.send(EngineEvent::Done);
                    return Ok(());
                }
            }

            info!("Executing {} tools in parallel (streaming)", parallel.len());
            let parallel_results = self.execute_tools_parallel(&parallel, event_tx).await;
            for outcome in parallel_results {
                self.record_completed_tool_outcome(outcome, event_tx).await;
            }
        }

        for tool_call in &sequential {
            if let Some(token) = cancel_token {
                if token.is_cancelled() {
                    self.complete_tool_turn_artifact();
                    let _ = event_tx.send(EngineEvent::Done);
                    return Ok(());
                }
            }

            let outcome = self
                .handle_tool_call(tool_call, event_tx, confirm_rx, cancel_token)
                .await?;
            self.record_completed_tool_outcome(outcome, event_tx).await;
        }

        Ok(())
    }
}
