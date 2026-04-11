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

    pub(super) fn complete_turn_runtime_artifact(
        &mut self,
        stop_reason: Option<&yode_llm::types::StopReason>,
    ) {
        let duration_ms = self
            .current_turn_started_at
            .take()
            .map(|started| started.elapsed().as_millis() as u64);
        self.last_turn_duration_ms = duration_ms;
        self.last_turn_stop_reason = stop_reason.map(|reason| format!("{:?}", reason));

        let dir = self.context.working_dir_compat().join(".yode").join("turns");
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let short_session = self.context.session_id.chars().take(8).collect::<String>();
        let path = dir.join(format!("{}-latest-turn.json", short_session));
        let payload = serde_json::json!({
            "session_id": self.context.session_id,
            "query_source": format!("{:?}", self.current_query_source),
            "duration_ms": self.last_turn_duration_ms,
            "stop_reason": self.last_turn_stop_reason,
            "tool_calls": self.tool_call_count,
            "tool_output_bytes": self.total_tool_results_bytes,
            "message_count": self.messages.len(),
            "completed_at": Self::now_timestamp(),
        });
        if std::fs::write(
            &path,
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
        )
        .is_ok()
        {
            self.last_turn_artifact_path = Some(path.display().to_string());
        }
    }
}
