use super::*;
use yode_llm::types::{AnthropicRequestHints, ProviderRequestHints};

impl AgentEngine {
    pub(super) fn build_chat_request(&self) -> ChatRequest {
        let tool_pool = self.build_tool_pool_snapshot();
        let request = ChatRequest {
            model: self.context.model.clone(),
            messages: self.messages.clone(),
            tools: convert_tool_definitions(&self.tools, Some(&tool_pool)),
            temperature: Some(0.7),
            max_tokens: Some(self.context.get_max_tokens()),
            provider_hints: self.build_provider_request_hints(),
        };
        if std::env::var("YODE_DEBUG_CHAT_REQUESTS").is_ok_and(|value| value == "1") {
            self.write_chat_request_debug_artifact(&request);
        }
        request
    }

    fn write_chat_request_debug_artifact(&self, request: &ChatRequest) {
        let debug_dir = self
            .context
            .working_dir_compat()
            .join(".yode")
            .join("debug")
            .join("chat-requests");
        if let Err(err) = std::fs::create_dir_all(&debug_dir) {
            warn!("Failed to create chat request debug dir: {}", err);
            return;
        }

        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        let session_id = self
            .context
            .session_id
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
            .take(36)
            .collect::<String>();
        let path = debug_dir.join(format!("{timestamp_ms}-{session_id}.json"));
        let messages = request
            .messages
            .iter()
            .enumerate()
            .map(|(index, message)| {
                json!({
                    "index": index,
                    "role": format!("{:?}", message.role),
                    "content": message.content,
                    "reasoning": message.reasoning,
                    "content_blocks": message.content_blocks,
                    "tool_calls": message.tool_calls,
                    "tool_call_id": message.tool_call_id,
                    "image_count": message.images.len(),
                })
            })
            .collect::<Vec<_>>();
        let payload = json!({
            "created_at": chrono::Local::now().to_rfc3339(),
            "session_id": self.context.session_id,
            "working_dir": self.context.working_dir_compat(),
            "provider": self.context.provider,
            "model": request.model,
            "message_count": request.messages.len(),
            "tool_count": request.tools.len(),
            "tool_names": request.tools.iter().map(|tool| tool.name.as_str()).collect::<Vec<_>>(),
            "tools": request.tools.iter().map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                    "annotations": tool.annotations,
                })
            }).collect::<Vec<_>>(),
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
            "provider_hints": request.provider_hints,
            "messages": messages,
        });

        match serde_json::to_string_pretty(&payload) {
            Ok(rendered) => {
                if let Err(err) = std::fs::write(&path, rendered) {
                    warn!("Failed to write chat request debug artifact: {}", err);
                } else {
                    info!("Wrote chat request debug artifact: {}", path.display());
                }
            }
            Err(err) => warn!("Failed to serialize chat request debug artifact: {}", err),
        }
    }

    pub(super) fn active_cache_edit_refs(&self) -> (Vec<String>, Vec<String>) {
        let visible_tool_results = self
            .messages
            .iter()
            .filter(|message| matches!(message.role, Role::Tool))
            .filter_map(|message| message.tool_call_id.clone())
            .collect::<std::collections::HashSet<_>>();

        let mut pending = self
            .pending_cache_edit_refs
            .iter()
            .filter(|cache_ref| visible_tool_results.contains(*cache_ref))
            .cloned()
            .collect::<Vec<_>>();
        pending.sort();
        pending.dedup();

        let mut pinned = self
            .pinned_cache_edit_refs
            .iter()
            .filter(|cache_ref| visible_tool_results.contains(*cache_ref))
            .filter(|cache_ref| !pending.contains(*cache_ref))
            .cloned()
            .collect::<Vec<_>>();
        pinned.sort();
        pinned.dedup();

        (pending, pinned)
    }

    fn build_provider_request_hints(&self) -> ProviderRequestHints {
        let (pending_deleted_cache_references, pinned_deleted_cache_references) =
            self.active_cache_edit_refs();
        let restore_system_blocks = self.request_restore_system_blocks();

        ProviderRequestHints {
            anthropic: self
                .supports_anthropic_cache_editing()
                .then_some(AnthropicRequestHints {
                    enable_prompt_caching: true,
                    pending_deleted_cache_references,
                    pinned_deleted_cache_references,
                }),
            restore_system_blocks,
        }
    }

    pub(super) fn supports_anthropic_cache_editing(&self) -> bool {
        let provider = self.provider.name().to_ascii_lowercase();
        let model = self.context.model.to_ascii_lowercase();
        provider.contains("anthropic") || model.contains("claude")
    }

    pub(super) fn build_partial_stream_assistant_message(
        &self,
        full_text: &str,
        full_reasoning: &str,
    ) -> Option<Message> {
        if full_text.is_empty() && full_reasoning.is_empty() {
            return None;
        }

        let mut blocks = Vec::new();
        if !full_reasoning.is_empty() {
            blocks.push(yode_llm::types::ContentBlock::Thinking {
                thinking: full_reasoning.to_string(),
                signature: None,
            });
        }
        if !full_text.is_empty() {
            blocks.push(yode_llm::types::ContentBlock::Text {
                text: full_text.to_string(),
            });
        }

        Some(
            Message {
                role: Role::Assistant,
                content: (!full_text.is_empty()).then(|| full_text.to_string()),
                reasoning: (!full_reasoning.is_empty()).then(|| full_reasoning.to_string()),
                content_blocks: blocks,
                tool_calls: vec![],
                tool_call_id: None,
                images: Vec::new(),
            }
            .normalized(),
        )
    }
}
