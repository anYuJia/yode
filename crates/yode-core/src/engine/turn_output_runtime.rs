use super::*;

impl AgentEngine {
    pub(super) fn push_and_persist_assistant_message(&mut self, message: &Message) {
        self.messages.push(message.clone());
        let tc_json = if !message.tool_calls.is_empty() {
            serde_json::to_string(&message.tool_calls).ok()
        } else {
            None
        };
        self.persist_message(
            "assistant",
            message.content.as_deref(),
            message.reasoning.as_deref(),
            tc_json.as_deref(),
            None,
        );
    }

    pub(super) async fn record_completed_tool_outcome(
        &mut self,
        outcome: ToolExecutionOutcome,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) {
        let tool_call = outcome.tool_call;
        let tool_call_id = tool_call.id.clone();
        let tool_name = tool_call.name.clone();
        let result = self
            .finalize_tool_result(
                &tool_call,
                outcome.result,
                outcome.started_at,
                outcome.duration_ms,
                outcome.progress_updates,
                outcome.parallel_batch,
            )
            .await;
        self.messages
            .push(Message::tool_result(&tool_call_id, &result.content));
        self.persist_message(
            "tool",
            Some(&result.content),
            None,
            None,
            Some(&tool_call_id),
        );

        let _ = event_tx.send(EngineEvent::ToolResult {
            id: tool_call_id,
            name: tool_name,
            result,
        });
    }
}
