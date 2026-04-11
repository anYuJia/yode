use super::*;
use std::sync::Arc;

use yode_tools::registry::ToolRegistry;
use yode_tools::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

mod compaction;
mod hooks;
mod partition;
mod runtime;

/// Minimal mock LLM provider (never actually called in these tests).
pub(super) struct MockProvider;

#[async_trait::async_trait]
impl yode_llm::provider::LlmProvider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }

    async fn chat(
        &self,
        _req: yode_llm::types::ChatRequest,
    ) -> anyhow::Result<yode_llm::types::ChatResponse> {
        unimplemented!("Mock provider should not be called in unit tests")
    }

    async fn chat_stream(
        &self,
        _req: yode_llm::types::ChatRequest,
        _tx: tokio::sync::mpsc::Sender<yode_llm::types::StreamEvent>,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }

    async fn list_models(&self) -> anyhow::Result<Vec<yode_llm::ModelInfo>> {
        Ok(vec![])
    }
}

/// A mock read-only tool for testing parallel execution.
pub(super) struct MockReadTool {
    pub(super) name: String,
}

#[async_trait::async_trait]
impl Tool for MockReadTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "mock read tool"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: true,
        }
    }

    async fn execute(
        &self,
        _params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> anyhow::Result<ToolResult> {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        Ok(ToolResult::success(format!("result from {}", self.name)))
    }
}

/// A mock write tool that requires confirmation.
pub(super) struct MockWriteTool {
    pub(super) name: String,
}

#[async_trait::async_trait]
impl Tool for MockWriteTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "mock write tool"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: true,
            supports_auto_execution: false,
            read_only: false,
        }
    }

    async fn execute(
        &self,
        _params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> anyhow::Result<ToolResult> {
        Ok(ToolResult::success("write done".to_string()))
    }
}

pub(super) struct MockPathTool;

#[async_trait::async_trait]
impl Tool for MockPathTool {
    fn name(&self) -> &str {
        "mock_path"
    }

    fn description(&self) -> &str {
        "mock path tool"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": ["path"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: false,
        }
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> anyhow::Result<ToolResult> {
        let path = params
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("missing");
        Ok(ToolResult::success(format!("path={}", path)))
    }
}

pub(super) fn make_engine(tools: Vec<Arc<dyn Tool>>, confirm_tools: Vec<String>) -> AgentEngine {
    let registry = ToolRegistry::new();
    for t in tools {
        registry.register(t);
    }
    let provider: Arc<dyn yode_llm::provider::LlmProvider> = Arc::new(MockProvider);
    let permissions = PermissionManager::from_confirmation_list(confirm_tools);
    let workdir = std::env::temp_dir().join(format!("yode-engine-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&workdir).unwrap();
    let context = AgentContext::new(workdir, "mock".to_string(), "claude-sonnet-4".to_string());
    AgentEngine::new(provider, Arc::new(registry), permissions, context)
}
