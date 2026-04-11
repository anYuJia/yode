use super::*;

impl AgentEngine {
    pub(super) fn build_chat_request(&self) -> ChatRequest {
        let tool_pool = self.build_tool_pool_snapshot();
        ChatRequest {
            model: self.context.model.clone(),
            messages: self.messages.clone(),
            tools: convert_tool_definitions(&self.tools, Some(&tool_pool)),
            temperature: Some(0.7),
            max_tokens: Some(self.context.get_max_tokens()),
        }
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
