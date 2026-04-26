use super::*;
use yode_llm::types::{AnthropicRequestHints, ProviderRequestHints};

impl AgentEngine {
    pub(super) fn build_chat_request(&self) -> ChatRequest {
        let tool_pool = self.build_tool_pool_snapshot();
        ChatRequest {
            model: self.context.model.clone(),
            messages: self.messages.clone(),
            tools: convert_tool_definitions(&self.tools, Some(&tool_pool)),
            temperature: Some(0.7),
            max_tokens: Some(self.context.get_max_tokens()),
            provider_hints: self.build_provider_request_hints(),
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
            anthropic: self.supports_anthropic_cache_editing().then_some(AnthropicRequestHints {
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
