use super::*;

pub(in crate::engine) enum StreamFinalizeAction {
    Continue,
    ReturnOk,
    Break,
}

impl AgentEngine {
    pub(super) async fn finalize_stream_turn(
        &mut self,
        mut buffers: StreamTurnBuffers,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
        confirm_rx: &mut mpsc::UnboundedReceiver<ConfirmResponse>,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<StreamFinalizeAction> {
        let mut assistant_msg = Message {
            role: Role::Assistant,
            content: if buffers.full_text.is_empty() {
                None
            } else {
                Some(buffers.full_text.clone())
            },
            reasoning: if buffers.full_reasoning.is_empty() {
                None
            } else {
                Some(buffers.full_reasoning.clone())
            },
            content_blocks: Vec::new(),
            tool_calls: buffers.tool_calls.clone(),
            tool_call_id: None,
            images: Vec::new(),
        };

        if let Some(response) = buffers.final_response.as_ref() {
            if response.stop_reason == Some(yode_llm::types::StopReason::MaxTokens) {
                let warning = "\n\n[WARNING: Response truncated due to max_tokens limit. Consider increasing effort level if more detail is needed.]";
                if let Some(content) = &mut assistant_msg.content {
                    content.push_str(warning);
                    buffers.full_text = content.clone();
                } else {
                    assistant_msg.content = Some(warning.to_string());
                    buffers.full_text = warning.to_string();
                }
                warn!("LLM streaming response truncated due to max_tokens");
            } else if (response.stop_reason == Some(yode_llm::types::StopReason::StopSequence)
                || matches!(
                    response.stop_reason,
                    Some(yode_llm::types::StopReason::Other(_))
                ))
                && (buffers.full_text.contains("[tool_") || buffers.full_text.contains("<tool_"))
            {
                warn!(
                    "LLM streaming response stopped via stop sequence or other reason but contains incomplete tool tags. Reason: {:?}",
                    response.stop_reason
                );
            }
        }

        if let Some(action) = self
            .handle_protocol_violation(&mut assistant_msg, &mut buffers.tool_calls, event_tx)
            .await?
        {
            return Ok(action);
        }

        if let Some(text) = assistant_msg.content.as_ref() {
            if self.is_protocol_violation(text) {
                assistant_msg.content = Some(self.clean_assistant_response(text));
                buffers.full_text = assistant_msg.content.as_ref().unwrap().clone();
            }
        }

        if !buffers.full_reasoning.is_empty() {
            assistant_msg
                .content_blocks
                .push(yode_llm::types::ContentBlock::Thinking {
                    thinking: buffers.full_reasoning.clone(),
                    signature: None,
                });
        }
        if !buffers.full_text.is_empty() {
            assistant_msg
                .content_blocks
                .push(yode_llm::types::ContentBlock::Text {
                    text: buffers.full_text.clone(),
                });
        }

        assistant_msg.normalize_in_place();
        self.push_and_persist_assistant_message(&assistant_msg);

        if !buffers.tool_calls.is_empty() {
            self.execute_stream_tool_calls(&buffers.tool_calls, event_tx, confirm_rx, cancel_token)
                .await?;
            return Ok(StreamFinalizeAction::Continue);
        }

        if let Some(mut response) = buffers.final_response {
            if response.message.tool_calls.is_empty() {
                if let Some(content) = response.message.content.clone() {
                    if content.len() > 3000 {
                        let trimmed: String = content.chars().take(1800).collect();
                        response.message.content = Some(format!(
                            "{}\n\n[Output truncated by runtime guard to keep response responsive. Ask to continue if you want more details.]",
                            trimmed
                        ));
                    }
                }
            }
            if response.message.content.is_none() && !buffers.full_text.is_empty() {
                response.message.content = Some(buffers.full_text.clone());
            }
            if response.message.reasoning.is_none() && !buffers.full_reasoning.is_empty() {
                response.message.reasoning = Some(buffers.full_reasoning.clone());
            }

            let content_for_analysis = response.message.content.clone();
            if let Some(content) = content_for_analysis {
                if content.contains("[tool_use") || content.contains("[DUMMY_TOOL") {
                    response.message.content = Some(self.clean_assistant_response(&content));
                }

                if response.message.tool_calls.is_empty() && content.contains("[tool_use") {
                    warn!(
                        "Detected leaked tool-use text; skipping text-recovery to avoid invalid tool schema propagation"
                    );
                }
            }

            self.push_and_persist_assistant_message(&response.message);

            if response.message.tool_calls.is_empty() {
                debug!("Streaming turn complete with no tool calls; finishing turn.");
                self.maybe_refresh_live_session_memory(Some(event_tx));
                self.complete_tool_turn_artifact();
                self.complete_turn_runtime_artifact(response.stop_reason.as_ref());
                let _ = event_tx.send(EngineEvent::TurnComplete(response));
                let _ = event_tx.send(EngineEvent::Done);
                return Ok(StreamFinalizeAction::Break);
            }

            debug!(
                "Streaming turn produced {} tool calls; continuing loop.",
                response.message.tool_calls.len()
            );
            let _ = event_tx.send(EngineEvent::TurnComplete(response));
            return Ok(StreamFinalizeAction::Continue);
        }

        self.complete_tool_turn_artifact();
        self.complete_turn_runtime_artifact(None);
        let _ = event_tx.send(EngineEvent::Done);
        Ok(StreamFinalizeAction::Break)
    }
}
