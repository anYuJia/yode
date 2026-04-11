use super::*;

#[tokio::test]
async fn test_bash_simple_command() {
    let tool = BashTool;
    let params = json!({"command": "echo hello"});
    let result = tool.execute(params, &ToolContext::empty()).await.unwrap();
    assert!(!result.is_error);
    assert!(result.content.contains("hello"));
}

#[tokio::test]
async fn test_bash_failing_command() {
    let tool = BashTool;
    let params = json!({"command": "exit 1"});
    let result = tool.execute(params, &ToolContext::empty()).await.unwrap();
    assert!(result.is_error);
    assert!(result.content.contains("exit code: 1"));
}

#[tokio::test]
async fn test_bash_timeout() {
    let tool = BashTool;
    let params = json!({"command": "sleep 10", "timeout": 1});
    let result = tool.execute(params, &ToolContext::empty()).await.unwrap();
    assert!(result.is_error);
    assert!(result.content.contains("timed out") || result.content.contains("Timeout"));
}

#[tokio::test]
async fn test_bash_stderr() {
    let tool = BashTool;
    let params = json!({"command": "echo err >&2"});
    let result = tool.execute(params, &ToolContext::empty()).await.unwrap();
    assert!(!result.is_error);
    assert!(result.content.contains("err"));
}

#[tokio::test]
async fn test_bash_background() {
    let tool = BashTool;
    let params = json!({"command": "sleep 0.1", "run_in_background": true});
    let result = tool.execute(params, &ToolContext::empty()).await.unwrap();
    assert!(!result.is_error);
    assert!(result.content.contains("background"));
}

#[tokio::test]
async fn test_bash_background_registers_runtime_task() {
    let tool = BashTool;
    let dir = tempfile::tempdir().unwrap();
    let mut ctx = ToolContext::empty();
    ctx.working_dir = Some(dir.path().to_path_buf());
    ctx.runtime_tasks = Some(std::sync::Arc::new(tokio::sync::Mutex::new(
        crate::runtime_tasks::RuntimeTaskStore::new(),
    )));

    let params = json!({"command": "echo hello", "run_in_background": true});
    let result = tool.execute(params, &ctx).await.unwrap();
    assert!(!result.is_error);
    let task_id = result
        .metadata
        .as_ref()
        .and_then(|meta| meta.get("task_id"))
        .and_then(|value| value.as_str())
        .unwrap()
        .to_string();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let tasks = ctx.runtime_tasks.as_ref().unwrap().lock().await.list();
    assert!(tasks.iter().any(|task| task.id == task_id));
}
