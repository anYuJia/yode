use tokio::sync::mpsc;
use yode_llm::providers::anthropic::AnthropicProvider;
use yode_llm::provider::LlmProvider;
use yode_llm::types::{ChatRequest, Message, StreamEvent, ToolDefinition};

#[tokio::test]
async fn test_anthropic_chat() {
    let api_key = std::env::var("ANTHROPIC_AUTH_TOKEN")
        .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
        .expect("Set ANTHROPIC_AUTH_TOKEN or ANTHROPIC_API_KEY");
    let base_url = std::env::var("ANTHROPIC_BASE_URL")
        .unwrap_or_else(|_| "https://api.anthropic.com".to_string());
    let model = std::env::var("ANTHROPIC_MODEL")
        .unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string());

    let provider = AnthropicProvider::new("anthropic", api_key, base_url);
    assert_eq!(provider.name(), "anthropic");

    // Test non-streaming chat
    let request = ChatRequest {
        model: model.clone(),
        messages: vec![
            Message::system("你是一个简洁的助手，用中文回答。"),
            Message::user("1+1等于几？只回答数字。"),
        ],
        tools: vec![],
        temperature: Some(0.0),
        max_tokens: Some(32),
    };

    let response = provider.chat(request).await.expect("chat should succeed");
    let text = response.message.content.expect("should have text content");
    println!("[非流式] 回复: {}", text);
    println!("[非流式] 模型: {}", response.model);
    println!("[非流式] 用量: {:?}", response.usage);
    assert!(text.contains("2"), "response should contain '2', got: {}", text);
}

#[tokio::test]
async fn test_anthropic_stream() {
    let api_key = std::env::var("ANTHROPIC_AUTH_TOKEN")
        .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
        .expect("Set ANTHROPIC_AUTH_TOKEN or ANTHROPIC_API_KEY");
    let base_url = std::env::var("ANTHROPIC_BASE_URL")
        .unwrap_or_else(|_| "https://api.anthropic.com".to_string());
    let model = std::env::var("ANTHROPIC_MODEL")
        .unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string());

    let provider = AnthropicProvider::new("anthropic", api_key, base_url);

    let request = ChatRequest {
        model,
        messages: vec![
            Message::system("你是一个简洁的助手。"),
            Message::user("用一句话说'你好世界'"),
        ],
        tools: vec![],
        temperature: Some(0.0),
        max_tokens: Some(64),
    };

    let (tx, mut rx) = mpsc::channel::<StreamEvent>(256);
    provider.chat_stream(request, tx).await.expect("stream should succeed");

    let mut full_text = String::new();
    while let Some(event) = rx.recv().await {
        match event {
            StreamEvent::TextDelta(delta) => {
                print!("{}", delta);
                full_text.push_str(&delta);
            }
            StreamEvent::Done(resp) => {
                println!("\n[流式] 完成，模型: {}, 用量: {:?}", resp.model, resp.usage);
            }
            StreamEvent::Error(e) => {
                panic!("Stream error: {}", e);
            }
            _ => {}
        }
    }
    println!("[流式] 完整回复: {}", full_text);
    assert!(!full_text.is_empty(), "stream should produce text");
}

#[tokio::test]
async fn test_anthropic_tool_call() {
    let api_key = std::env::var("ANTHROPIC_AUTH_TOKEN")
        .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
        .expect("Set ANTHROPIC_AUTH_TOKEN or ANTHROPIC_API_KEY");
    let base_url = std::env::var("ANTHROPIC_BASE_URL")
        .unwrap_or_else(|_| "https://api.anthropic.com".to_string());
    let model = std::env::var("ANTHROPIC_MODEL")
        .unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string());

    let provider = AnthropicProvider::new("anthropic", api_key, base_url);

    let tools = vec![ToolDefinition {
        name: "read_file".to_string(),
        description: "读取文件内容".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "文件路径"
                }
            },
            "required": ["file_path"]
        }),
    }];

    let request = ChatRequest {
        model,
        messages: vec![
            Message::system("你是一个编程助手。当需要读取文件时，使用 read_file 工具。"),
            Message::user("请读取 /tmp/test.txt 文件的内容"),
        ],
        tools,
        temperature: Some(0.0),
        max_tokens: Some(256),
    };

    let response = provider.chat(request).await.expect("chat should succeed");
    println!("[工具调用] 文本: {:?}", response.message.content);
    println!("[工具调用] 工具调用数: {}", response.message.tool_calls.len());
    for tc in &response.message.tool_calls {
        println!("[工具调用] name={}, id={}, args={}", tc.name, tc.id, tc.arguments);
    }
    assert!(!response.message.tool_calls.is_empty(), "should trigger tool call");
    assert_eq!(response.message.tool_calls[0].name, "read_file");
}
