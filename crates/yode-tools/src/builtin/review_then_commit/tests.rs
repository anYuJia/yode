use std::pin::Pin;
use std::process::Command;
use std::sync::Arc;

use serde_json::json;

use super::ReviewThenCommitTool;
use crate::builtin::review_common::review_output_has_findings;
use crate::tool::{SubAgentOptions, SubAgentRunner, Tool, ToolContext};

struct MockRunner {
    output: String,
}

impl SubAgentRunner for MockRunner {
    fn run_sub_agent(
        &self,
        _prompt: String,
        _options: SubAgentOptions,
    ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<String>> + Send + '_>> {
        let output = self.output.clone();
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

#[test]
fn review_findings_heuristic_respects_clean_output() {
    assert!(!review_output_has_findings(
        "No issues found.\nResidual risk: none."
    ));
    assert!(review_output_has_findings("1. Missing test for edge case"));
}

#[tokio::test]
async fn review_then_commit_commits_when_review_is_clean() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    std::fs::write(dir.path().join("a.txt"), "hello").unwrap();

    let mut ctx = ToolContext::empty();
    ctx.working_dir = Some(dir.path().to_path_buf());
    ctx.sub_agent_runner = Some(Arc::new(MockRunner {
        output: "No issues found.\nResidual risk: none.".to_string(),
    }));

    let tool = ReviewThenCommitTool;
    let result = tool
        .execute(
            json!({
                "message": "add a.txt",
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
async fn review_then_commit_aborts_on_findings() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    std::fs::write(dir.path().join("a.txt"), "hello").unwrap();

    let mut ctx = ToolContext::empty();
    ctx.working_dir = Some(dir.path().to_path_buf());
    ctx.sub_agent_runner = Some(Arc::new(MockRunner {
        output: "1. Missing regression test".to_string(),
    }));

    let tool = ReviewThenCommitTool;
    let result = tool
        .execute(
            json!({
                "message": "add a.txt",
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
