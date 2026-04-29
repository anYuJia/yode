use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::provider::LlmProvider;
use crate::types::{ChatRequest, ChatResponse, ModelInfo, StreamEvent};

#[derive(Debug, Clone, Default)]
pub struct MockProvider {
    name: String,
    state: Arc<Mutex<MockProviderState>>,
}

#[derive(Debug, Default)]
struct MockProviderState {
    chat_responses: VecDeque<Result<ChatResponse, String>>,
    stream_events: VecDeque<Vec<StreamEvent>>,
    models: Vec<ModelInfo>,
    requests: Vec<ChatRequest>,
}

impl MockProvider {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            state: Arc::new(Mutex::new(MockProviderState::default())),
        }
    }

    pub fn with_chat_response(self, response: ChatResponse) -> Self {
        self.state
            .lock()
            .unwrap()
            .chat_responses
            .push_back(Ok(response));
        self
    }

    pub fn with_chat_error(self, error: impl Into<String>) -> Self {
        self.state
            .lock()
            .unwrap()
            .chat_responses
            .push_back(Err(error.into()));
        self
    }

    pub fn with_stream_events(self, events: Vec<StreamEvent>) -> Self {
        self.state.lock().unwrap().stream_events.push_back(events);
        self
    }

    pub fn with_models(self, models: Vec<ModelInfo>) -> Self {
        self.state.lock().unwrap().models = models;
        self
    }

    pub fn requests(&self) -> Vec<ChatRequest> {
        self.state.lock().unwrap().requests.clone()
    }
}

#[async_trait]
impl LlmProvider for MockProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let mut state = self.state.lock().unwrap();
        state.requests.push(request);
        state
            .chat_responses
            .pop_front()
            .ok_or_else(|| anyhow!("No mock chat response available"))
            .and_then(|result| result.map_err(|error| anyhow!(error)))
    }

    async fn chat_stream(&self, request: ChatRequest, tx: mpsc::Sender<StreamEvent>) -> Result<()> {
        let events = {
            let mut state = self.state.lock().unwrap();
            state.requests.push(request);
            state
                .stream_events
                .pop_front()
                .ok_or_else(|| anyhow!("No mock stream events available"))?
        };
        for event in events {
            tx.send(event)
                .await
                .map_err(|_| anyhow!("Mock stream receiver dropped"))?;
        }
        Ok(())
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        Ok(self.state.lock().unwrap().models.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{stream_done, Message, Usage};

    fn response(text: &str) -> ChatResponse {
        ChatResponse {
            message: Message::assistant(text),
            usage: Usage::default(),
            model: "mock-model".to_string(),
            stop_reason: None,
        }
    }

    fn request() -> ChatRequest {
        ChatRequest {
            model: "mock-model".to_string(),
            messages: vec![Message::user("hello")],
            tools: Vec::new(),
            temperature: None,
            max_tokens: None,
            provider_hints: Default::default(),
        }
    }

    #[tokio::test]
    async fn mock_provider_returns_queued_chat_responses_and_records_requests() {
        let provider = MockProvider::new("mock").with_chat_response(response("done"));

        let result = provider.chat(request()).await.unwrap();

        assert_eq!(provider.name(), "mock");
        assert_eq!(result.message.content.as_deref(), Some("done"));
        assert_eq!(provider.requests().len(), 1);
    }

    #[tokio::test]
    async fn mock_provider_returns_queued_chat_errors() {
        let provider = MockProvider::new("mock").with_chat_error("rate limited");

        let error = provider.chat(request()).await.unwrap_err();

        assert!(error.to_string().contains("rate limited"));
        assert_eq!(provider.requests().len(), 1);
    }

    #[tokio::test]
    async fn mock_provider_streams_queued_events() {
        let provider = MockProvider::new("mock").with_stream_events(vec![
            StreamEvent::TextDelta("hi".to_string()),
            stream_done(
                Message::assistant("hi"),
                Usage::default(),
                "mock-model".to_string(),
                None,
            ),
        ]);
        let (tx, mut rx) = mpsc::channel(4);

        provider.chat_stream(request(), tx).await.unwrap();

        assert!(matches!(rx.recv().await, Some(StreamEvent::TextDelta(text)) if text == "hi"));
        assert!(matches!(rx.recv().await, Some(StreamEvent::Done(_))));
        assert_eq!(provider.requests().len(), 1);
    }
}
