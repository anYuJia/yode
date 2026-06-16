use super::*;
use std::sync::Arc;

use yode_tools::registry::ToolRegistry;
use yode_tools::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

mod compaction;
mod hooks;
mod partition;
mod runtime;
mod stream_recovery;

#[test]
fn ordered_recent_read_files_prefers_last_access_order() {
    let mut files_read = std::collections::HashMap::new();
    files_read.insert("src/older.rs".to_string(), 1);
    files_read.insert("src/recent.rs".to_string(), 2);
    files_read.insert("src/untracked_order.rs".to_string(), 3);
    let recent_file_reads = vec![
        "src/older.rs".to_string(),
        "src/recent.rs".to_string(),
        "src/older.rs".to_string(),
    ];

    let ordered = ordered_recent_read_files(&recent_file_reads, &files_read);

    assert_eq!(ordered[0], "src/older.rs");
    assert_eq!(ordered[1], "src/recent.rs");
    assert!(ordered.contains(&"src/untracked_order.rs".to_string()));
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
    let provider: Arc<dyn yode_llm::provider::LlmProvider> =
        Arc::new(yode_llm::MockProvider::new("mock"));
    let permissions = PermissionManager::from_confirmation_list(confirm_tools);
    let workdir = std::env::temp_dir().join(format!("yode-engine-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&workdir).unwrap();
    let context = AgentContext::new(workdir, "mock".to_string(), "claude-sonnet-4".to_string());
    AgentEngine::new(provider, Arc::new(registry), permissions, context)
}

#[test]
fn stream_error_event_is_not_forwarded_directly_to_ui() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut full_text = String::new();
    let mut pending_text = String::new();
    let mut full_reasoning = String::new();
    let mut tool_calls = Vec::new();
    let mut final_response = None;

    AgentEngine::process_stream_event(
        yode_llm::types::StreamEvent::Error("connection reset".to_string()),
        &mut full_text,
        &mut pending_text,
        &mut full_reasoning,
        &mut tool_calls,
        &mut final_response,
        &tx,
    );

    assert!(rx.try_recv().is_err());
}

#[test]
fn action_narrative_is_emitted_and_removed_from_text_stream() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut full_text = String::new();
    let mut pending_text = String::new();
    let mut full_reasoning = String::new();
    let mut tool_calls = Vec::new();
    let mut final_response = None;

    AgentEngine::process_stream_event(
        yode_llm::types::StreamEvent::TextDelta(
            "开始 <action_narrative>我先查看项目结构，确认入口".to_string(),
        ),
        &mut full_text,
        &mut pending_text,
        &mut full_reasoning,
        &mut tool_calls,
        &mut final_response,
        &tx,
    );
    AgentEngine::process_stream_event(
        yode_llm::types::StreamEvent::TextDelta("</action_narrative>继续".to_string()),
        &mut full_text,
        &mut pending_text,
        &mut full_reasoning,
        &mut tool_calls,
        &mut final_response,
        &tx,
    );

    assert_eq!(full_text, "开始 继续");
    assert_eq!(pending_text, "");

    match rx.try_recv().unwrap() {
        EngineEvent::TextDelta(text) => assert_eq!(text, "开始 "),
        other => panic!("expected text delta, got {:?}", other),
    }
    match rx.try_recv().unwrap() {
        EngineEvent::ActionNarrative(text) => assert_eq!(text, "我先查看项目结构，确认入口"),
        other => panic!("expected action narrative, got {:?}", other),
    }
    match rx.try_recv().unwrap() {
        EngineEvent::TextDelta(text) => assert_eq!(text, "继续"),
        other => panic!("expected text delta, got {:?}", other),
    }
    assert!(rx.try_recv().is_err());
}

#[test]
fn action_narrative_is_removed_from_done_response_content() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut full_text = String::new();
    let mut pending_text = String::new();
    let mut full_reasoning = String::new();
    let mut tool_calls = Vec::new();
    let mut final_response = None;

    AgentEngine::process_stream_event(
        yode_llm::types::stream_done(
            yode_llm::types::Message::assistant(
                "<action_narrative>我会验证构建结果。</action_narrative>完成",
            ),
            yode_llm::types::Usage::default(),
            "mock".to_string(),
            None,
        ),
        &mut full_text,
        &mut pending_text,
        &mut full_reasoning,
        &mut tool_calls,
        &mut final_response,
        &tx,
    );

    assert!(rx.try_recv().is_err());
    let response = final_response.expect("final response");
    assert_eq!(response.message.content.as_deref(), Some("完成"));
}

#[test]
fn action_narrative_tags_inside_reasoning_stay_private_reasoning() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut full_text = String::new();
    let mut pending_text = String::new();
    let mut full_reasoning = String::new();
    let mut tool_calls = Vec::new();
    let mut final_response = None;

    AgentEngine::process_stream_event(
        yode_llm::types::StreamEvent::ReasoningDelta(
            "<action_narrative>我先查看项目结构。</action_narrative>".to_string(),
        ),
        &mut full_text,
        &mut pending_text,
        &mut full_reasoning,
        &mut tool_calls,
        &mut final_response,
        &tx,
    );

    assert_eq!(
        full_reasoning,
        "<action_narrative>我先查看项目结构。</action_narrative>"
    );
    match rx.try_recv().unwrap() {
        EngineEvent::ReasoningDelta(text) => {
            assert_eq!(
                text,
                "<action_narrative>我先查看项目结构。</action_narrative>"
            )
        }
        other => panic!("expected reasoning delta, got {:?}", other),
    }
    assert!(rx.try_recv().is_err());
}

#[test]
fn streaming_tool_call_start_is_not_forwarded_before_execution() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut full_text = String::new();
    let mut pending_text = String::new();
    let mut full_reasoning = String::new();
    let mut tool_calls = Vec::new();
    let mut final_response = None;

    AgentEngine::process_stream_event(
        yode_llm::types::StreamEvent::ToolCallStart {
            id: "call_1".to_string(),
            name: "read_file".to_string(),
        },
        &mut full_text,
        &mut pending_text,
        &mut full_reasoning,
        &mut tool_calls,
        &mut final_response,
        &tx,
    );
    AgentEngine::process_stream_event(
        yode_llm::types::StreamEvent::ToolCallDelta {
            id: "call_1".to_string(),
            arguments: r#"{"file_path":"Cargo.toml","action_narrative":"我先看配置入口。"}"#
                .to_string(),
        },
        &mut full_text,
        &mut pending_text,
        &mut full_reasoning,
        &mut tool_calls,
        &mut final_response,
        &tx,
    );

    assert_eq!(tool_calls.len(), 1);
    assert!(tool_calls[0].arguments.contains("action_narrative"));
    assert!(rx.try_recv().is_err());
}
