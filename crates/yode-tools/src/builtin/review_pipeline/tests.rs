use std::pin::Pin;
use std::process::Command;
use std::sync::{Arc, Mutex};

use serde_json::json;

use super::ReviewPipelineTool;
use crate::tool::{SubAgentOptions, SubAgentRunner, Tool, ToolContext};

struct QueueRunner {
    outputs: Arc<Mutex<Vec<String>>>,
}

impl SubAgentRunner for QueueRunner {
    fn run_sub_agent(
        &self,
        _prompt: String,
        _options: SubAgentOptions,
    ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<String>> + Send + '_>> {
        let output = self.outputs.lock().unwrap().remove(0);
        Box::pin(async move { Ok(output) })
    }
}

fn init_repo(dir: &std::path::Path) {
    Command::new("git")
        .args(["init"])
        .current_dir(dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(dir)
        .output()
        .unwrap();
}

#[tokio::test]
async fn review_pipeline_commits_when_review_and_verification_are_clean() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    std::fs::write(dir.path().join("a.txt"), "hello").unwrap();

    let mut ctx = ToolContext::empty();
    ctx.working_dir = Some(dir.path().to_path_buf());
    ctx.sub_agent_runner = Some(Arc::new(QueueRunner {
        outputs: Arc::new(Mutex::new(vec![
            "No issues found.\nResidual risk: none.".to_string(),
            "No issues found.\nResidual risk: none.".to_string(),
        ])),
    }));

    let tool = ReviewPipelineTool;
    let result = tool
        .execute(
            json!({
                "focus": "current changes",
                "commit_message": "add a.txt",
                "files": ["a.txt"]
            }),
            &ctx,
        )
        .await
        .unwrap();

    assert!(!result.is_error, "{}", result.content);
    let log = Command::new("git")
        .args(["log", "--oneline"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let log_str = String::from_utf8_lossy(&log.stdout);
    assert!(log_str.contains("add a.txt"));
}

#[tokio::test]
async fn review_pipeline_stops_on_findings() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    std::fs::write(dir.path().join("a.txt"), "hello").unwrap();

    let mut ctx = ToolContext::empty();
    ctx.working_dir = Some(dir.path().to_path_buf());
    ctx.sub_agent_runner = Some(Arc::new(QueueRunner {
        outputs: Arc::new(Mutex::new(vec![
            "1. Missing regression test".to_string(),
            "No issues found.".to_string(),
        ])),
    }));

    let tool = ReviewPipelineTool;
    let result = tool
        .execute(
            json!({
                "focus": "current changes",
                "commit_message": "add a.txt",
                "files": ["a.txt"]
            }),
            &ctx,
        )
        .await
        .unwrap();

    assert!(result.is_error);
    let log = Command::new("git")
        .args(["log", "--oneline"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let log_str = String::from_utf8_lossy(&log.stdout);
    assert!(!log_str.contains("add a.txt"));
}
