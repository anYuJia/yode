use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::types::{ChatRequest, ChatResponse, ModelInfo, StreamEvent};

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse>;

    async fn chat_stream(
        &self,
        request: ChatRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<()>;

    async fn list_models(&self) -> Result<Vec<ModelInfo>>;
}
