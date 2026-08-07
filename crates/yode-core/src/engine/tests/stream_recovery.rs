use super::*;

#[tokio::test]
async fn test_handle_interrupted_stream_persists_partial_message() {
    let mut engine = make_engine(vec![], vec![]);
    let (tx, mut rx) = mpsc::unbounded_channel();
    let buffers = super::super::streaming_turn_runtime::StreamTurnBuffers {
        full_text: "partial text".to_string(),
        pending_text: String::new(),
        full_reasoning: "partial reasoning".to_string(),
        tool_calls: Vec::new(),
        final_response: None,
    };

    let handled = engine
        .handle_interrupted_stream(false, true, &buffers, &tx)
        .await;

    assert!(handled);
    assert!(engine
        .messages
        .iter()
        .any(|message| message.content.as_deref() == Some("partial text")));
    assert!(matches!(
        rx.recv().await,
        Some(EngineEvent::TextComplete(_))
    ));
}

#[tokio::test]
async fn test_handle_interrupted_stream_ignores_normal_flow() {
    let mut engine = make_engine(vec![], vec![]);
    let (tx, _rx) = mpsc::unbounded_channel();
    let buffers = super::super::streaming_turn_runtime::StreamTurnBuffers::default();

    let handled = engine
        .handle_interrupted_stream(false, false, &buffers, &tx)
        .await;

    assert!(!handled);
    assert_eq!(engine.messages.len(), 1);
}

/// STREAM-001：截断流（无 final_response）即使携带工具调用也绝不能执行。
#[tokio::test]
async fn truncated_stream_tool_calls_are_discarded_not_executed() {
    use yode_llm::types::ToolCall;

    let mut engine = make_engine(
        vec![Arc::new(MockWriteTool {
            name: "mock_write".into(),
        })],
        vec![],
    );
    let (tx, _rx) = mpsc::unbounded_channel();
    let (_, mut confirm_rx) = mpsc::unbounded_channel::<ConfirmResponse>();
    let buffers = super::super::streaming_turn_runtime::StreamTurnBuffers {
        full_text: "partial".to_string(),
        pending_text: String::new(),
        full_reasoning: String::new(),
        tool_calls: vec![ToolCall {
            id: "tc-truncated".to_string(),
            name: "mock_write".to_string(),
            // 半截 JSON 参数：绝不能被执行
            arguments: r#"{"file_path": "/tmp/evil""#.to_string(),
        }],
        final_response: None,
    };

    let action = engine
        .finalize_stream_turn(buffers, &tx, &mut confirm_rx, None)
        .await
        .unwrap();

    // 必须中断而不是继续执行
    assert!(matches!(
        action,
        super::super::streaming_turn_runtime::finalization::StreamFinalizeAction::Break
    ));
    // 截断流不持久化任何助手消息（含半截工具调用）
    assert_eq!(
        engine
            .messages
            .iter()
            .filter(|message| matches!(message.role, yode_llm::types::Role::Assistant))
            .count(),
        0
    );
    // 没有任何工具被修改
    assert!(!engine.files_modified.iter().any(|p| p.contains("evil")));
}

/// STREAM-001：完整流（final_response 存在）的工具调用正常执行。
#[tokio::test]
async fn completed_stream_tool_calls_execute_normally() {
    use yode_llm::types::{ChatResponse, Message, Role, ToolCall};

    let mut engine = make_engine(
        vec![Arc::new(MockWriteTool {
            name: "mock_write".into(),
        })],
        vec![],
    );
    let (tx, _rx) = mpsc::unbounded_channel();
    let (_, mut confirm_rx) = mpsc::unbounded_channel::<ConfirmResponse>();
    let buffers = super::super::streaming_turn_runtime::StreamTurnBuffers {
        full_text: String::new(),
        pending_text: String::new(),
        full_reasoning: String::new(),
        tool_calls: vec![ToolCall {
            id: "tc-complete".to_string(),
            name: "mock_write".to_string(),
            arguments: "{}".to_string(),
        }],
        final_response: Some(ChatResponse {
            message: Message {
                role: Role::Assistant,
                content: None,
                reasoning: None,
                content_blocks: Vec::new(),
                tool_calls: vec![],
                tool_call_id: None,
                images: Vec::new(),
            },
            usage: Default::default(),
            stop_reason: Some(yode_llm::types::StopReason::ToolUse),
            model: "mock".to_string(),
        }),
    };

    let action = engine
        .finalize_stream_turn(buffers, &tx, &mut confirm_rx, None)
        .await
        .unwrap();

    assert!(matches!(
        action,
        super::super::streaming_turn_runtime::finalization::StreamFinalizeAction::Continue
    ));
}
