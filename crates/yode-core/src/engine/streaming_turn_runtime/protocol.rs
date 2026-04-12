use super::*;

impl AgentEngine {
    pub(in crate::engine) async fn handle_protocol_violation(
        &mut self,
        assistant_msg: &mut Message,
        tool_calls: &mut Vec<ToolCall>,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) -> Result<Option<finalization::StreamFinalizeAction>> {
        let Some(full_text) = assistant_msg.content.as_ref() else {
            self.violation_retries = 0;
            return Ok(None);
        };

        if assistant_msg.tool_calls.is_empty()
            && !full_text.is_empty()
            && self.is_protocol_violation(full_text)
        {
            let recovered = self.recover_leaked_tool_calls(full_text);
            if !recovered.is_empty() {
                info!(
                    "Recovered {} leaked tool calls from text response. Proceeding with execution.",
                    recovered.len()
                );
                assistant_msg.tool_calls = recovered;
                *tool_calls = assistant_msg.tool_calls.clone();
                self.violation_retries = 0;
                return Ok(None);
            } else if self.violation_retries < 2 {
                self.violation_retries += 1;
                warn!(
                    "Protocol violation detected (attempt {}). Retrying with strict constraints...",
                    self.violation_retries
                );
                let _ = event_tx.send(EngineEvent::Thinking);

                self.messages.push(Message::user(
                    "STRICT PROTOCOL VIOLATION: You outputted internal tool tags ([tool_use], [DUMMY_TOOL], etc.) in your text response. \
                     This is forbidden. Please respond again using ONLY natural language. Do NOT use tool tags or JSON in this response."
                ));
                return Ok(Some(finalization::StreamFinalizeAction::Continue));
            } else {
                let error_message = "Critical protocol failure: Model repeatedly outputted internal tool tags in text field. Aborting to prevent loop.";
                error!("{}", error_message);
                let _ = event_tx.send(EngineEvent::Error(error_message.to_string()));
                let _ = event_tx.send(EngineEvent::Done);
                return Ok(Some(finalization::StreamFinalizeAction::ReturnOk));
            }
        }

        self.violation_retries = 0;
        Ok(None)
    }
}
