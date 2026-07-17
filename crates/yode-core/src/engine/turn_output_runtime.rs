use super::*;

impl AgentEngine {
    pub(super) fn push_and_persist_assistant_message(&mut self, message: &Message) {
        self.messages.push(message.clone());
        let tc_json = if !message.tool_calls.is_empty() {
            match serde_json::to_string(&message.tool_calls) {
                Ok(json) => Some(json),
                Err(err) => {
                    tracing::warn!(
                        session_id = %self.context.session_id,
                        tool_call_count = message.tool_calls.len(),
                        error = %err,
                        "Failed to serialize assistant tool_calls for persistence; storing without tool_calls_json"
                    );
                    None
                }
            }
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
        self.persist_message_with_metadata(
            "tool",
            Some(&result.content),
            None,
            None,
            Some(&tool_call_id),
            result.metadata.as_ref(),
        );

        let _ = event_tx.send(EngineEvent::ToolResult {
            id: tool_call_id,
            name: tool_name,
            result,
        });
    }

    pub(super) async fn complete_turn_runtime_artifact(
        &mut self,
        stop_reason: Option<&yode_llm::types::StopReason>,
    ) {
        let duration_ms = self
            .current_turn_started_at
            .take()
            .map(|started| started.elapsed().as_millis() as u64);
        self.last_turn_duration_ms = duration_ms;
        self.last_turn_stop_reason = stop_reason.map(|reason| format!("{:?}", reason));

        let dir = self
            .context
            .working_dir_compat()
            .join(".yode")
            .join("turns");
        if let Err(err) = tokio::fs::create_dir_all(&dir).await {
            tracing::warn!(
                "Failed to create turn artifact directory {}: {}",
                dir.display(),
                err
            );
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
        let body = match serde_json::to_string_pretty(&payload) {
            Ok(body) => body,
            Err(err) => {
                tracing::warn!("Failed to serialize turn artifact payload: {}", err);
                return;
            }
        };
        match tokio::fs::write(&path, body).await {
            Ok(()) => {
                self.last_turn_artifact_path = Some(path.display().to_string());
            }
            Err(err) => {
                tracing::warn!("Failed to write turn artifact {}: {}", path.display(), err);
            }
        }
    }

    pub(super) async fn run_stop_hooks_before_turn_complete(
        &mut self,
        response: &yode_llm::types::ChatResponse,
    ) -> bool {
        let Some(hook_mgr) = self.hook_manager.as_ref() else {
            return false;
        };

        let assistant_text = response.message.content.as_deref().unwrap_or_default();
        let hook_ctx = HookContext {
            event: HookEvent::Stop.to_string(),
            session_id: self.context.session_id.clone(),
            working_dir: self.context.working_dir_compat().display().to_string(),
            tool_name: None,
            tool_input: None,
            tool_output: None,
            error: None,
            user_prompt: None,
            metadata: Some(serde_json::json!({
                "query_source": format!("{:?}", self.current_query_source),
                "stop_reason": response.stop_reason.as_ref().map(|reason| format!("{:?}", reason)),
                "assistant_text": assistant_text,
                "message_count": self.messages.len(),
                "tool_calls_this_turn": self.tool_call_count,
                "tool_output_bytes_this_turn": self.total_tool_results_bytes,
                "runtime": self.runtime_hook_metadata(),
            })),
        };

        let results = hook_mgr.execute(HookEvent::Stop, &hook_ctx).await;
        let mut continuation_parts = Vec::new();
        let mut advisory_parts = Vec::new();
        let mut requested_continue = false;
        let mut continuation_reason = None;

        for result in results {
            if result.blocked || result.deferred {
                requested_continue = true;
                if let Some(reason) = result.reason.as_deref() {
                    if continuation_reason.is_none() {
                        continuation_reason = Some(reason.to_string());
                    }
                    continuation_parts.push(format!("- Reason: {}", reason));
                }
            }

            if let Some(stdout) = result.stdout {
                let trimmed = stdout.trim();
                if !trimmed.is_empty() {
                    if result.blocked || result.deferred {
                        continuation_parts.push(trimmed.to_string());
                    } else {
                        advisory_parts.push(trimmed.to_string());
                    }
                }
            }
        }

        if !advisory_parts.is_empty() {
            let message = format!(
                "[System Auto-Context via stop hooks]\n{}",
                advisory_parts.join("\n\n")
            );
            self.messages.push(Message::system(&message));
            self.persist_message("system", Some(&message), None, None, None);
        }

        self.append_hook_wake_notifications_as_system_message();

        if !requested_continue {
            return false;
        }

        if self.stop_hook_continue_attempted {
            warn!("stop hook requested continuation again; ignoring to avoid a loop");
            return false;
        }
        self.stop_hook_continue_attempted = true;
        self.stop_hook_continue_count = self.stop_hook_continue_count.saturating_add(1);
        self.last_stop_hook_continue_reason = continuation_reason
            .clone()
            .or_else(|| Some("stop hook requested continuation without a reason".to_string()));

        if continuation_parts.is_empty() {
            continuation_parts.push(
                "- Stop hook requested another assistant step before finalizing.".to_string(),
            );
        }

        let message = format!(
            "[Stop hook requested continuation]\n{}\n\nContinue the turn by addressing the stop hook feedback. Do not repeat the same final answer unchanged.",
            continuation_parts.join("\n\n")
        );
        self.messages.push(Message::system(&message));
        self.persist_message("system", Some(&message), None, None, None);
        true
    }
}
