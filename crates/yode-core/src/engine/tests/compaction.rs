use super::*;
use std::sync::{Arc, Mutex as StdMutex};

use yode_llm::types::ImageData;
use yode_tools::registry::ToolRegistry;

enum SummaryStep {
    Error(&'static str),
    Summary(&'static str),
}

struct StubSummaryProvider {
    steps: Arc<StdMutex<Vec<SummaryStep>>>,
}

#[async_trait::async_trait]
impl yode_llm::provider::LlmProvider for StubSummaryProvider {
    fn name(&self) -> &str {
        "stub-summary"
    }

    async fn chat(
        &self,
        req: yode_llm::types::ChatRequest,
    ) -> anyhow::Result<yode_llm::types::ChatResponse> {
        let step = self.steps.lock().unwrap().remove(0);
        match step {
            SummaryStep::Error(message) => Err(anyhow::anyhow!(message)),
            SummaryStep::Summary(content) => Ok(yode_llm::types::ChatResponse {
                message: Message::assistant(content),
                usage: yode_llm::types::Usage::default(),
                model: req.model,
                stop_reason: None,
            }),
        }
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

fn make_engine_with_provider(provider: Arc<dyn yode_llm::provider::LlmProvider>) -> AgentEngine {
    let registry = ToolRegistry::new();
    let permissions = PermissionManager::from_confirmation_list(vec![]);
    let workdir = std::env::temp_dir().join(format!("yode-engine-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&workdir).unwrap();
    let context = AgentContext::new(workdir, "stub".to_string(), "claude-sonnet-4".to_string());
    AgentEngine::new(provider, Arc::new(registry), permissions, context)
}

#[tokio::test]
async fn test_autocompact_circuit_breaker_trips_after_repeated_failures() {
    let mut engine = make_engine(vec![], vec![]);
    engine.messages = vec![
        Message::system("system"),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::user("u3"),
        Message::assistant("a3"),
    ];
    engine.current_query_source = QuerySource::User;

    let (tx, mut rx) = mpsc::unbounded_channel();
    for _ in 0..MAX_CONSECUTIVE_COMPACTION_FAILURES {
        engine.maybe_compact_context(190_000, &tx).await;
    }

    assert!(engine.autocompact_disabled);
    assert_eq!(
        engine.compaction_failures,
        MAX_CONSECUTIVE_COMPACTION_FAILURES
    );
    assert!(engine.messages.iter().any(|msg| {
        msg.content
            .as_deref()
            .unwrap_or_default()
            .contains("Auto-compact disabled")
    }));
    let runtime = engine.runtime_state();
    assert_eq!(
        runtime.last_compaction_breaker_reason.as_deref(),
        Some("compression made no changes")
    );
    assert!(matches!(rx.try_recv(), Ok(EngineEvent::Error(_))));
}

#[tokio::test]
async fn test_autocompact_prefers_session_memory_summary_when_available() {
    let mut engine = make_engine(vec![], vec![]);
    let project_root = engine.context().working_dir_compat();
    let live_path = crate::session_memory::live_session_memory_path(&project_root);
    std::fs::create_dir_all(live_path.parent().unwrap()).unwrap();
    std::fs::write(
        &live_path,
        "# Session Snapshot\n\n## 2026-01-01 session abc123\n\n- Goals: stabilize compact flow\n- Findings: older tool results dominate token usage\n- Decisions: use session memory first\n",
    )
    .unwrap();

    let big = "x".repeat(18_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user(&big),
        Message::assistant(&big),
        Message::tool_result("tc1", &big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
        Message::user("recent7"),
        Message::assistant("recent8"),
    ];
    engine.current_query_source = QuerySource::User;

    let (tx, _rx) = mpsc::unbounded_channel();
    engine.maybe_compact_context(190_000, &tx).await;

    assert!(engine.messages.iter().any(|message| {
        message
            .content
            .as_deref()
            .unwrap_or_default()
            .contains("persisted session memory")
    }));
    let runtime = engine.runtime_state();
    assert_eq!(runtime.last_compaction_mode.as_deref(), Some("auto"));
    assert_eq!(
        runtime
            .compaction_cause_histogram
            .get("success_auto_session_memory"),
        Some(&1)
    );
}

#[test]
fn test_recovery_artifact_written_on_state_transition() {
    let mut engine = make_engine(vec![], vec![]);
    engine.last_failed_signature = Some("bash:{\"command\":\"cargo test\"}".to_string());
    engine.error_buckets.insert(ToolErrorType::Validation, 2);
    engine.update_recovery_state();

    assert_eq!(engine.recovery_state, RecoveryState::SingleStepMode);
    let artifact = engine
        .last_recovery_artifact_path
        .as_ref()
        .expect("recovery artifact should exist");
    let content = std::fs::read_to_string(artifact).unwrap();
    assert!(content.contains("SingleStepMode"));
    assert!(content.contains("Breadcrumbs"));
}

#[tokio::test]
async fn test_compact_query_source_skips_autocompact() {
    let mut engine = make_engine(vec![], vec![]);
    let big = "x".repeat(18_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user(&big),
        Message::assistant(&big),
        Message::tool_result("tc1", &big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
    ];
    engine.current_query_source = QuerySource::Compact;

    let before_len = engine.messages.len();
    let (tx, _rx) = mpsc::unbounded_channel();
    engine.maybe_compact_context(190_000, &tx).await;

    assert_eq!(engine.messages.len(), before_len);
    assert_eq!(engine.compaction_failures, 0);
    assert!(!engine.messages.iter().any(|msg| {
        msg.content
            .as_deref()
            .unwrap_or_default()
            .starts_with("[Context summary]")
    }));
}

#[tokio::test]
async fn test_force_compact_ignores_auto_compact_guard() {
    let mut engine = make_engine(vec![], vec![]);
    let big = "x".repeat(18_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user(&big),
        Message::assistant(&big),
        Message::tool_result("tc1", &big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
    ];
    engine.current_query_source = QuerySource::Compact;
    engine.autocompact_disabled = true;

    let (tx, _rx) = mpsc::unbounded_channel();
    let changed = engine.force_compact_keep_last(2, tx).await;

    assert!(changed);
    let runtime = engine.runtime_state();
    assert_eq!(runtime.total_compactions, 1);
    assert_eq!(runtime.manual_compactions, 1);
    assert!(
        engine.messages.len() < 12
            || engine.messages.iter().any(|msg| {
                msg.content
                    .as_deref()
                    .unwrap_or_default()
                    .contains("[compressed]")
            })
    );
}

#[test]
fn test_prompt_too_long_errors_trigger_reactive_compact_detection() {
    let engine = make_engine(vec![], vec![]);
    let err = anyhow::anyhow!(
        "OpenAI API error (400): This model's maximum context length is 128000 tokens."
    );
    assert!(engine.should_reactive_compact_error(&err));
}

#[test]
fn test_media_errors_trigger_reactive_strip_detection() {
    let engine = make_engine(vec![], vec![]);
    let err = anyhow::anyhow!("Anthropic API error (400): image exceeds maximum size");
    assert!(engine.should_reactive_strip_media_error(&err));
}

#[test]
fn test_reactive_strip_old_media_clears_images_from_older_messages() {
    let mut engine = make_engine(vec![], vec![]);
    engine.messages = vec![
        Message::system("system"),
        Message::user_with_images(
            "older image",
            vec![ImageData {
                base64: "abcd".to_string(),
                media_type: "image/png".to_string(),
            }],
        ),
        Message::assistant("a1"),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
        Message::user("recent7"),
        Message::assistant("recent8"),
    ];

    let changed = engine.reactive_strip_old_media();

    assert!(changed);
    assert!(engine.messages[1].images.is_empty());
    assert!(engine.messages[1]
        .content
        .as_deref()
        .unwrap_or_default()
        .contains("older media removed"));
}

#[test]
fn test_reactive_compact_prefers_prefix_range_from_token_gap() {
    let mut engine = make_engine(vec![], vec![]);
    let big = "x".repeat(8_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user(&big),
        Message::assistant(&big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
        Message::user("recent7"),
        Message::assistant("recent8"),
    ];

    let end = engine
        .reactive_prefix_end_for_token_gap("prompt is too long: 16000 tokens > 12000 maximum")
        .expect("should derive a reactive prefix");
    assert!(end > 1);
    assert!(end < engine.messages.len().saturating_sub(6));
}

#[tokio::test]
async fn test_partial_compact_up_to_keeps_newer_tail() {
    let mut engine = make_engine(vec![], vec![]);
    engine.messages = vec![
        Message::system("system"),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::user("u3"),
        Message::assistant("a3"),
        Message::user("u4"),
        Message::assistant("a4"),
    ];

    let (tx, _rx) = mpsc::unbounded_channel();
    let changed = engine.force_partial_compact_up_to(4, tx).await;

    assert!(changed);
    assert!(engine.messages.iter().any(|message| {
        message
            .content
            .as_deref()
            .unwrap_or_default()
            .starts_with("[Context summary]")
    }));
    assert!(engine
        .messages
        .iter()
        .any(|message| message.content.as_deref() == Some("u3")));
    assert!(engine
        .messages
        .iter()
        .any(|message| message.content.as_deref() == Some("a4")));
}

#[tokio::test]
async fn test_partial_compact_from_keeps_older_prefix() {
    let mut engine = make_engine(vec![], vec![]);
    engine.messages = vec![
        Message::system("system"),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::user("u3"),
        Message::assistant("a3"),
        Message::user("u4"),
        Message::assistant("a4"),
    ];

    let (tx, _rx) = mpsc::unbounded_channel();
    let changed = engine.force_partial_compact_from(5, tx).await;

    assert!(changed);
    assert_eq!(engine.messages[1].content.as_deref(), Some("u1"));
    assert_eq!(engine.messages[2].content.as_deref(), Some("a1"));
    assert!(engine.messages.iter().any(|message| {
        message
            .content
            .as_deref()
            .unwrap_or_default()
            .starts_with("[Context summary]")
    }));
    assert!(!engine.messages.iter().any(|message| {
        message
            .content
            .as_deref()
            .unwrap_or_default()
            .starts_with("[Post-compact restore:")
    }));

    let request = engine.build_chat_request();
    assert!(!request.provider_hints.restore_system_blocks.is_empty());
}

#[tokio::test]
async fn test_repeated_compaction_does_not_duplicate_restore_blocks() {
    let mut engine = make_engine(vec![], vec![]);
    let big = "x".repeat(18_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user(&big),
        Message::assistant(&big),
        Message::tool_result("tc1", &big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
    ];

    let (tx, _rx) = mpsc::unbounded_channel();
    assert!(engine.force_compact_keep_last(4, tx.clone()).await);
    assert!(engine.force_compact_keep_last(4, tx).await);

    let restore_count = engine
        .messages
        .iter()
        .filter(|message| {
            message
                .content
                .as_deref()
                .unwrap_or_default()
                .starts_with("[Post-compact restore:")
        })
        .count();
    assert_eq!(restore_count, 0);

    let request = engine.build_chat_request();
    let request_restore_count = request.provider_hints.restore_system_blocks.len();
    assert!(request_restore_count <= 7);
}

#[tokio::test]
async fn test_manual_compaction_uses_llm_structured_summary_when_available() {
    let provider: Arc<dyn yode_llm::provider::LlmProvider> =
        Arc::new(StubSummaryProvider {
            steps: Arc::new(StdMutex::new(vec![SummaryStep::Summary(
                "## Goals\n- finish compaction parity\n## Current State\n- context compacted\n## Findings\n- older tool results were dominating\n## Decisions\n- use structured summary\n## Files\n- src/engine.rs\n## Tools\n- read_file\n## Constraints\n- keep it concise\n## Open Questions\n- None\n## Next Steps\n- continue with recent tail",
            )])),
        });
    let mut engine = make_engine_with_provider(provider);
    engine.set_model("gpt-3.5".to_string());
    let big = "x".repeat(18_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user(&big),
        Message::assistant(&big),
        Message::tool_result("tc1", &big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
    ];

    let (tx, _rx) = mpsc::unbounded_channel();
    let changed = engine.force_compact(tx).await;

    assert!(changed);
    assert!(engine.messages.iter().any(|message| {
        message
            .content
            .as_deref()
            .unwrap_or_default()
            .contains("LLM-generated structured summary")
    }));
    assert!(engine.messages.iter().any(|message| {
        message
            .content
            .as_deref()
            .unwrap_or_default()
            .contains("## Goals")
    }));
}

#[tokio::test]
async fn test_manual_compaction_retries_llm_summary_after_prompt_too_long() {
    let steps = Arc::new(StdMutex::new(vec![
        SummaryStep::Error("prompt too long"),
        SummaryStep::Summary(
            "## Goals\n- recover compaction summary\n## Current State\n- retry succeeded\n## Findings\n- head truncation worked\n## Decisions\n- retry once\n## Files\n- None\n## Tools\n- None\n## Constraints\n- concise\n## Open Questions\n- None\n## Next Steps\n- continue",
        ),
    ]));
    let provider: Arc<dyn yode_llm::provider::LlmProvider> = Arc::new(StubSummaryProvider {
        steps: Arc::clone(&steps),
    });
    let mut engine = make_engine_with_provider(provider);
    engine.set_model("gpt-3.5".to_string());
    let big = "x".repeat(18_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user(&big),
        Message::assistant(&big),
        Message::tool_result("tc1", &big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
    ];

    let (tx, _rx) = mpsc::unbounded_channel();
    let changed = engine.force_compact_keep_last(2, tx).await;

    assert!(changed);
    let remaining = steps.lock().unwrap().len();
    assert_eq!(remaining, 0, "remaining summary steps: {}", remaining);
    assert!(engine.messages.iter().any(|message| {
        message
            .content
            .as_deref()
            .unwrap_or_default()
            .contains("retry succeeded")
    }));
}
