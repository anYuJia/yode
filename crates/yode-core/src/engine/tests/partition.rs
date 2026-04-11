use super::*;

use yode_llm::types::ToolCall;

#[test]
fn test_partition_all_read_only() {
    let engine = make_engine(
        vec![
            Arc::new(MockReadTool { name: "r1".into() }),
            Arc::new(MockReadTool { name: "r2".into() }),
            Arc::new(MockReadTool { name: "r3".into() }),
        ],
        vec![],
    );
    let tcs = vec![
        ToolCall {
            id: "1".into(),
            name: "r1".into(),
            arguments: "{}".into(),
        },
        ToolCall {
            id: "2".into(),
            name: "r2".into(),
            arguments: "{}".into(),
        },
        ToolCall {
            id: "3".into(),
            name: "r3".into(),
            arguments: "{}".into(),
        },
    ];
    let (par, seq) = engine.partition_tool_calls(&tcs);
    assert_eq!(par.len(), 3);
    assert_eq!(seq.len(), 0);
}

#[test]
fn test_partition_mixed() {
    let engine = make_engine(
        vec![
            Arc::new(MockReadTool {
                name: "reader".into(),
            }),
            Arc::new(MockWriteTool),
        ],
        vec!["mock_write".into()],
    );
    let tcs = vec![
        ToolCall {
            id: "1".into(),
            name: "reader".into(),
            arguments: "{}".into(),
        },
        ToolCall {
            id: "2".into(),
            name: "mock_write".into(),
            arguments: "{}".into(),
        },
        ToolCall {
            id: "3".into(),
            name: "reader".into(),
            arguments: "{}".into(),
        },
    ];
    let (par, seq) = engine.partition_tool_calls(&tcs);
    assert_eq!(par.len(), 2);
    assert_eq!(seq.len(), 1);
    assert_eq!(seq[0].name, "mock_write");
}

#[test]
fn test_partition_unknown_tool() {
    let engine = make_engine(vec![], vec![]);
    let tcs = vec![ToolCall {
        id: "1".into(),
        name: "nonexistent".into(),
        arguments: "{}".into(),
    }];
    let (par, seq) = engine.partition_tool_calls(&tcs);
    assert_eq!(par.len(), 0);
    assert_eq!(seq.len(), 1);
}

#[test]
fn test_partition_read_only_needing_confirm() {
    let engine = make_engine(
        vec![Arc::new(MockReadTool {
            name: "sensitive".into(),
        })],
        vec!["sensitive".into()],
    );
    let tcs = vec![ToolCall {
        id: "1".into(),
        name: "sensitive".into(),
        arguments: "{}".into(),
    }];
    let (par, seq) = engine.partition_tool_calls(&tcs);
    assert_eq!(par.len(), 0, "Confirm-required tools must not be parallelized");
    assert_eq!(seq.len(), 1);
}

#[tokio::test]
async fn test_parallel_returns_all_results_in_order() {
    let mut engine = make_engine(
        vec![
            Arc::new(MockReadTool { name: "a".into() }),
            Arc::new(MockReadTool { name: "b".into() }),
            Arc::new(MockReadTool { name: "c".into() }),
        ],
        vec![],
    );
    let tcs = vec![
        ToolCall {
            id: "x1".into(),
            name: "a".into(),
            arguments: "{}".into(),
        },
        ToolCall {
            id: "x2".into(),
            name: "b".into(),
            arguments: "{}".into(),
        },
        ToolCall {
            id: "x3".into(),
            name: "c".into(),
            arguments: "{}".into(),
        },
    ];
    let (tx, mut rx) = mpsc::unbounded_channel();
    let results = engine.execute_tools_parallel(&tcs, &tx).await;

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].tool_call.id, "x1");
    assert_eq!(results[1].tool_call.id, "x2");
    assert_eq!(results[2].tool_call.id, "x3");
    for outcome in &results {
        assert!(!outcome.result.is_error);
    }

    let mut starts = 0;
    while let Ok(ev) = rx.try_recv() {
        if matches!(ev, EngineEvent::ToolCallStart { .. }) {
            starts += 1;
        }
    }
    assert_eq!(starts, 3);
}

#[tokio::test]
async fn test_parallel_empty() {
    let mut engine = make_engine(vec![], vec![]);
    let (tx, _rx) = mpsc::unbounded_channel();
    let results = engine.execute_tools_parallel(&[], &tx).await;
    assert!(results.is_empty());
}
