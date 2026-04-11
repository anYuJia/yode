use super::message::Message;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
    ContentFilter,
    Other(String),
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub message: Message,
    pub usage: Usage,
    pub model: String,
    pub stop_reason: Option<StopReason>,
}

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub cache_write_tokens: u32,
    pub cache_read_tokens: u32,
}

impl Usage {
    pub fn has_reported_tokens(&self) -> bool {
        self.prompt_tokens > 0
            || self.completion_tokens > 0
            || self.total_tokens > 0
            || self.cache_write_tokens > 0
            || self.cache_read_tokens > 0
    }

    pub fn uncached_prompt_tokens(&self) -> u32 {
        self.prompt_tokens
            .saturating_sub(self.cache_write_tokens)
            .saturating_sub(self.cache_read_tokens)
    }
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    TextDelta(String),
    ReasoningDelta(String),
    /// Real-time usage information (e.g. prompt tokens known at start)
    UsageUpdate(Usage),
    ToolCallStart { id: String, name: String },
    ToolCallDelta { id: String, arguments: String },
    ToolCallEnd { id: String },
    Done(ChatResponse),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
}

#[cfg(test)]
mod tests {
    use super::Usage;

    #[test]
    fn usage_exposes_uncached_prompt_tokens() {
        let usage = Usage {
            prompt_tokens: 1_000,
            completion_tokens: 80,
            total_tokens: 1_080,
            cache_write_tokens: 250,
            cache_read_tokens: 150,
        };

        assert!(usage.has_reported_tokens());
        assert_eq!(usage.uncached_prompt_tokens(), 600);
    }
}
