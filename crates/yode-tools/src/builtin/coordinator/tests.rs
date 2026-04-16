use super::planning::{
    build_execution_phases, normalize_workstreams, render_phase_timeline, NormalizedWorkstream,
};
use super::CoordinateAgentsTool;
use crate::tool::{SubAgentOptions, SubAgentRunner, Tool, ToolContext};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

struct MockRunner {
    seen: Arc<Mutex<Vec<(String, SubAgentOptions)>>>,
}

impl SubAgentRunner for MockRunner {
    fn run_sub_agent(
        &self,
        prompt: String,
        options: SubAgentOptions,
    ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<String>> + Send + '_>> {
        self.seen.lock().unwrap().push((prompt, options));
        Box::pin(async { Ok("done".to_string()) })
    }
}

#[tokio::test]
async fn coordinate_agents_runs_multiple_workstreams() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut ctx = ToolContext::empty();
    let dir = tempfile::tempdir().unwrap();
    ctx.working_dir = Some(dir.path().to_path_buf());
    ctx.sub_agent_runner = Some(Arc::new(MockRunner {
        seen: Arc::clone(&seen),
    }));

    let tool = CoordinateAgentsTool;
    let result = tool
        .execute(
            serde_json::json!({
                "goal": "ship the feature",
                "workstreams": [
                    {
                        "id": "review",
                        "description": "review",
                        "prompt": "review the patch"
                    },
                    {
                        "id": "verify",
                        "description": "verify",
                        "prompt": "run validation",
                        "depends_on": ["review"]
                    }
                ]
            }),
            &ctx,
        )
        .await
        .unwrap();

    assert!(!result.is_error);
    assert!(result.content.contains("\"status\": \"ok\""));
    assert_eq!(seen.lock().unwrap().len(), 2);
    assert!(result.content.contains("\"phase\": 1"));
    assert!(result.content.contains("\"phase\": 2"));
    let metadata = result.metadata.unwrap();
    assert!(metadata.get("team_id").and_then(|v| v.as_str()).is_some());
    let team_state = metadata
        .get("team_state_artifact")
        .and_then(|v| v.as_str())
        .unwrap();
    assert!(std::path::Path::new(team_state).exists());
}

#[tokio::test]
async fn coordinate_agents_dry_run_returns_phase_plan() {
    let dir = tempfile::tempdir().unwrap();
    let mut ctx = ToolContext::empty();
    ctx.working_dir = Some(dir.path().to_path_buf());

    let tool = CoordinateAgentsTool;
    let result = tool
        .execute(
            serde_json::json!({
                "goal": "ship the feature",
                "dry_run": true,
                "workstreams": [
                    {
                        "id": "review",
                        "description": "review",
                        "prompt": "review the patch"
                    },
                    {
                        "id": "verify",
                        "description": "verify",
                        "prompt": "run validation",
                        "depends_on": ["review"]
                    }
                ]
            }),
            &ctx,
        )
        .await
        .unwrap();

    assert!(!result.is_error);
    assert!(result.content.contains("Coordinator phase timeline"));
    assert!(result.content.contains("\"phase\": 1"));
    assert!(result.content.contains("\"phase\": 2"));
    let metadata = result.metadata.unwrap();
    assert_eq!(metadata["dry_run"], true);
    assert!(metadata.get("team_id").and_then(|v| v.as_str()).is_some());
}

#[test]
fn render_phase_timeline_groups_batches() {
    let phases = vec![
        vec![
            NormalizedWorkstream {
                id: "review".to_string(),
                description: "review".to_string(),
                prompt: "review".to_string(),
                subagent_type: None,
                model: None,
                run_in_background: Some(false),
                allowed_tools: vec![],
                depends_on: vec![],
            },
            NormalizedWorkstream {
                id: "verify".to_string(),
                description: "verify".to_string(),
                prompt: "verify".to_string(),
                subagent_type: None,
                model: None,
                run_in_background: Some(false),
                allowed_tools: vec![],
                depends_on: vec![],
            },
        ],
        vec![NormalizedWorkstream {
            id: "ship".to_string(),
            description: "ship".to_string(),
            prompt: "ship".to_string(),
            subagent_type: None,
            model: None,
            run_in_background: Some(false),
            allowed_tools: vec![],
            depends_on: vec!["review".to_string()],
        }],
    ];
    let timeline = render_phase_timeline(&phases, 1);
    assert!(timeline.contains("Phase 1"));
    assert!(timeline.contains("Batch 1: review (review)"));
    assert!(timeline.contains("Batch 2: verify (verify)"));
    assert!(timeline.contains("Phase 2"));
}

#[tokio::test]
async fn coordinate_agents_respects_max_parallel_batches() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut ctx = ToolContext::empty();
    ctx.sub_agent_runner = Some(Arc::new(MockRunner {
        seen: Arc::clone(&seen),
    }));

    let tool = CoordinateAgentsTool;
    let result = tool
        .execute(
            serde_json::json!({
                "goal": "ship the feature",
                "max_parallel": 2,
                "workstreams": [
                    { "id": "a", "description": "a", "prompt": "a" },
                    { "id": "b", "description": "b", "prompt": "b" },
                    { "id": "c", "description": "c", "prompt": "c" }
                ]
            }),
            &ctx,
        )
        .await
        .unwrap();

    assert!(!result.is_error);
    assert!(result.content.contains("\"batch\": 1"));
    assert!(result.content.contains("\"batch\": 2"));
    assert_eq!(result.metadata.unwrap()["max_parallel"], 2);
    assert_eq!(seen.lock().unwrap().len(), 3);
}

#[test]
fn coordinator_rejects_unknown_dependency() {
    let result = normalize_workstreams(vec![super::Workstream {
        id: Some("verify".to_string()),
        description: "verify".to_string(),
        prompt: "run validation".to_string(),
        subagent_type: None,
        model: None,
        run_in_background: None,
        allowed_tools: Vec::new(),
        depends_on: vec!["missing".to_string()],
    }]);
    assert!(result.is_err());
}

#[test]
fn coordinator_reports_blocked_cycle_details() {
    let workstreams = vec![
        NormalizedWorkstream {
            id: "a".to_string(),
            description: "a".to_string(),
            prompt: "a".to_string(),
            subagent_type: None,
            model: None,
            run_in_background: None,
            allowed_tools: Vec::new(),
            depends_on: vec!["b".to_string()],
        },
        NormalizedWorkstream {
            id: "b".to_string(),
            description: "b".to_string(),
            prompt: "b".to_string(),
            subagent_type: None,
            model: None,
            run_in_background: None,
            allowed_tools: Vec::new(),
            depends_on: vec!["a".to_string()],
        },
    ];

    let err = build_execution_phases(&workstreams).unwrap_err();
    assert!(err.to_string().contains("a -> waiting for b"));
    assert!(err.to_string().contains("b -> waiting for a"));
}
