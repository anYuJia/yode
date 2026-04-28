use anyhow::Context;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use serde_json::Value;

use crate::builtin::team_runtime::{
    hydrate_agent_team_manager, persist_agent_team_runtime, persist_agent_team_snapshot,
    update_agent_team_member, AgentTeamMemberState,
};
use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct AgentTool;

#[async_trait]
impl Tool for AgentTool {
    fn name(&self) -> &str {
        "agent"
    }

    fn user_facing_name(&self) -> &str {
        "Sub-Agent"
    }

    fn activity_description(&self, params: &Value) -> String {
        let desc = params
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("complex task");
        format!("Sub-agent working on: {}", desc)
    }

    fn description(&self) -> &str {
        "Launch a sub-agent to handle a complex task autonomously. The sub-agent runs with \
         its own conversation history and a subset of available tools. Use this for tasks \
         that benefit from independent exploration or parallel work."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "description": {
                    "type": "string",
                    "description": "A short (3-5 word) description of the task"
                },
                "prompt": {
                    "type": "string",
                    "description": "The task for the agent to perform"
                },
                "subagent_type": {
                    "type": "string",
                    "description": "The type of specialized agent to use for this task (e.g. 'plan', 'explore', 'verification')"
                },
                "model": {
                    "type": "string",
                    "enum": ["sonnet", "opus", "haiku"],
                    "description": "Optional model override for this agent. If omitted, inherits from the parent."
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Set to true to run this agent in the background. You will be notified when it completes."
                },
                "isolation": {
                    "type": "string",
                    "enum": ["worktree"],
                    "description": "Isolation mode. 'worktree' creates a temporary git worktree so the agent works on an isolated copy of the repo."
                },
                "cwd": {
                    "type": "string",
                    "description": "Absolute path to run the agent in. Overrides the working directory for all operations within this agent."
                },
                "team_id": {
                    "type": "string",
                    "description": "Optional team id used to attach this sub-agent to a live team runtime."
                },
                "team_name": {
                    "type": "string",
                    "description": "Alias for team_id. Use this when addressing a reusable named team."
                },
                "member_id": {
                    "type": "string",
                    "description": "Optional member id for this sub-agent inside the team runtime."
                },
                "name": {
                    "type": "string",
                    "description": "Alias for member_id. Use this to make the team member addressable by name."
                }
            },
            "required": ["prompt", "description"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: true,
            supports_auto_execution: false,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let prompt = params
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("'prompt' parameter is required"))?
            .to_string();

        let description = params
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("complex task")
            .to_string();

        let subagent_type = params
            .get("subagent_type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let model = params
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let run_in_background = params
            .get("run_in_background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let isolation = params
            .get("isolation")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let cwd = params
            .get("cwd")
            .and_then(|v| v.as_str())
            .map(std::path::PathBuf::from);

        let allowed_tools: Vec<String> = params
            .get("allowed_tools") // Backward compatibility for any internal calls
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let team_id = params
            .get("team_id")
            .or_else(|| params.get("team_name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let member_id = params
            .get("member_id")
            .or_else(|| params.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let allowed_tools_clone = allowed_tools.clone();
        let subagent_type_clone = subagent_type.clone();
        let model_clone = model.clone();

        let runner = ctx
            .sub_agent_runner
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Sub-agent runner not available"))?;

        let options = crate::tool::SubAgentOptions {
            description: description.clone(),
            subagent_type,
            model,
            run_in_background,
            isolation,
            cwd,
            allowed_tools,
            team_id: team_id.clone(),
            member_id: member_id.clone(),
        };
        let prompt = if let (Some(team_id), Some(member_id), Some(manager)) = (
            team_id.as_deref(),
            member_id.as_deref(),
            ctx.team_runtime.as_ref(),
        ) {
            let mailbox = {
                let mut manager = manager.lock().await;
                let mailbox = manager.consume_message_context(team_id, member_id, 6);
                if !mailbox.is_empty() {
                    if let (Some(working_dir), Some(snapshot)) =
                        (ctx.working_dir.as_deref(), manager.snapshot(team_id))
                    {
                        let _ = persist_agent_team_snapshot(working_dir, &snapshot);
                    }
                }
                mailbox
            };
            if mailbox.is_empty() {
                prompt
            } else {
                let mailbox_summary = mailbox
                    .iter()
                    .map(|message| {
                        format!(
                            "{} [{}:{}] {}",
                            message.at, message.target, message.kind, message.message
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("Team mailbox:\n{}\n\nTask:\n{}", mailbox_summary, prompt)
            }
        } else {
            prompt
        };

        match runner.run_sub_agent(prompt, options).await {
            Ok(result) => {
                let artifact_path = if !run_in_background {
                    ctx.working_dir
                        .as_deref()
                        .and_then(|dir| persist_sub_agent_artifact(dir, &description, &result).ok())
                        .map(|path| path.display().to_string())
                } else {
                    None
                };
                let team_artifacts = if let (Some(team_id), Some(working_dir)) =
                    (team_id.as_deref(), ctx.working_dir.as_deref())
                {
                    if let Some(manager) = ctx.team_runtime.as_ref() {
                        let snapshot = {
                            let mut manager = manager.lock().await;
                            if hydrate_agent_team_manager(working_dir, &mut manager, team_id)?
                                .is_none()
                            {
                                let state = manager.ensure_team(
                                    &description,
                                    Some(team_id),
                                    if run_in_background {
                                        "background"
                                    } else {
                                        "foreground"
                                    },
                                    vec![AgentTeamMemberState {
                                        member_id: member_id
                                            .clone()
                                            .unwrap_or_else(|| "member-1".to_string()),
                                        description: description.clone(),
                                        subagent_type: subagent_type_clone.clone(),
                                        model: model_clone.clone(),
                                        run_in_background,
                                        allowed_tools: allowed_tools_clone.clone(),
                                        permission_inheritance: if allowed_tools_clone.is_empty() {
                                            "parent_tool_pool".to_string()
                                        } else {
                                            "explicit_allowlist".to_string()
                                        },
                                        status: if run_in_background {
                                            "running".to_string()
                                        } else {
                                            "completed".to_string()
                                        },
                                        runtime_task_id: parse_background_task_id(&result),
                                        last_result_preview: Some(
                                            result.chars().take(240).collect(),
                                        ),
                                        result_artifact_path: artifact_path.clone(),
                                        last_updated_at: Some(
                                            chrono::Local::now()
                                                .format("%Y-%m-%d %H:%M:%S")
                                                .to_string(),
                                        ),
                                        pending_message_count: 0,
                                        last_message_at: None,
                                    }],
                                );
                                manager.upsert_snapshot(state, Vec::new());
                            }
                            let _ = manager.update_member(
                                team_id,
                                member_id.as_deref().unwrap_or("member-1"),
                                if run_in_background {
                                    "running"
                                } else {
                                    "completed"
                                },
                                parse_background_task_id(&result),
                                Some(result.clone()),
                                artifact_path.clone(),
                            );
                            manager.snapshot(team_id)
                        };
                        snapshot.as_ref().and_then(|snapshot| {
                            persist_agent_team_snapshot(working_dir, snapshot).ok()
                        })
                    } else {
                        let _ = persist_agent_team_runtime(
                            working_dir,
                            &description,
                            Some(team_id),
                            if run_in_background {
                                "background"
                            } else {
                                "foreground"
                            },
                            vec![AgentTeamMemberState {
                                member_id: member_id
                                    .clone()
                                    .unwrap_or_else(|| "member-1".to_string()),
                                description: description.clone(),
                                subagent_type: subagent_type_clone.clone(),
                                model: model_clone.clone(),
                                run_in_background,
                                allowed_tools: allowed_tools_clone.clone(),
                                permission_inheritance: if allowed_tools_clone.is_empty() {
                                    "parent_tool_pool".to_string()
                                } else {
                                    "explicit_allowlist".to_string()
                                },
                                status: if run_in_background {
                                    "running".to_string()
                                } else {
                                    "completed".to_string()
                                },
                                runtime_task_id: parse_background_task_id(&result),
                                last_result_preview: Some(result.chars().take(240).collect()),
                                result_artifact_path: artifact_path.clone(),
                                last_updated_at: Some(
                                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                                ),
                                pending_message_count: 0,
                                last_message_at: None,
                            }],
                        );
                        update_agent_team_member(
                            working_dir,
                            team_id,
                            member_id.as_deref().unwrap_or("member-1"),
                            if run_in_background {
                                "running"
                            } else {
                                "completed"
                            },
                            parse_background_task_id(&result),
                            Some(result.clone()),
                            artifact_path.clone(),
                        )
                        .ok()
                    }
                } else {
                    None
                };

                Ok(ToolResult::success_with_metadata(
                    if let Some(path) = &artifact_path {
                        format!("{}\n\nSub-agent artifact: {}", result, path)
                    } else {
                        result
                    },
                    json!({
                        "description": description,
                        "run_in_background": run_in_background,
                        "subagent_artifact_path": artifact_path,
                        "team_summary_artifact": team_artifacts
                            .as_ref()
                            .and_then(|set| set.summary_path.as_ref())
                            .map(|path| path.display().to_string()),
                        "team_state_artifact": team_artifacts
                            .as_ref()
                            .and_then(|set| set.state_path.as_ref())
                            .map(|path| path.display().to_string()),
                        "team_monitor_artifact": team_artifacts
                            .as_ref()
                            .and_then(|set| set.monitor_path.as_ref())
                            .map(|path| path.display().to_string()),
                        "team_id": team_id,
                        "member_id": member_id,
                    }),
                ))
            }
            Err(e) => Ok(ToolResult::error(format!("Sub-agent failed: {}", e))),
        }
    }
}

fn parse_background_task_id(output: &str) -> Option<String> {
    output
        .strip_prefix("Background sub-agent launched as ")
        .and_then(|rest| rest.split_once('.'))
        .map(|(task_id, _)| task_id.trim().to_string())
}

fn persist_sub_agent_artifact(
    working_dir: &std::path::Path,
    description: &str,
    body: &str,
) -> Result<std::path::PathBuf> {
    let dir = working_dir.join(".yode").join("agent-results");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create agent artifact dir: {}", dir.display()))?;

    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!("agent-{}.md", timestamp));
    let content = format!(
        "# Sub-Agent Result\n\n- Description: {}\n- Timestamp: {}\n\n## Result\n\n```text\n{}\n```\n",
        description,
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        body.trim()
    );
    std::fs::write(&path, content)
        .with_context(|| format!("Failed to write sub-agent artifact: {}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use yode_agent::AgentTeamManager;

    use super::AgentTool;
    use crate::tool::{SubAgentOptions, SubAgentRunner, Tool, ToolContext};
    use serde_json::json;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::Mutex as StdMutex;

    struct MockRunner;

    impl SubAgentRunner for MockRunner {
        fn run_sub_agent(
            &self,
            _prompt: String,
            _options: SubAgentOptions,
        ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<String>> + Send + '_>>
        {
            Box::pin(async { Ok("sub-agent done".to_string()) })
        }
    }

    struct CapturingRunner {
        prompt: Arc<StdMutex<Option<String>>>,
        options: Arc<StdMutex<Option<SubAgentOptions>>>,
    }

    impl SubAgentRunner for CapturingRunner {
        fn run_sub_agent(
            &self,
            prompt: String,
            options: SubAgentOptions,
        ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<String>> + Send + '_>>
        {
            *self.prompt.lock().unwrap() = Some(prompt);
            *self.options.lock().unwrap() = Some(options);
            Box::pin(async { Ok("sub-agent done".to_string()) })
        }
    }

    #[tokio::test]
    async fn agent_tool_persists_sync_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());
        ctx.sub_agent_runner = Some(Arc::new(MockRunner));

        let tool = AgentTool;
        let result = tool
            .execute(
                json!({
                    "description": "inspect code",
                    "prompt": "inspect the workspace",
                    "run_in_background": false
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        let artifact_path = result
            .metadata
            .as_ref()
            .and_then(|meta| meta.get("subagent_artifact_path"))
            .and_then(|value| value.as_str())
            .unwrap();
        assert!(artifact_path.contains(".yode/agent-results/agent-"));
        assert!(std::path::Path::new(artifact_path).exists());
        assert!(result.content.contains("Sub-agent artifact:"));
    }

    #[tokio::test]
    async fn agent_tool_updates_team_artifacts_when_team_is_provided() {
        let dir = tempfile::tempdir().unwrap();
        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());
        ctx.sub_agent_runner = Some(Arc::new(MockRunner));
        ctx.team_runtime = Some(Arc::new(tokio::sync::Mutex::new(AgentTeamManager::new())));

        let tool = AgentTool;
        let result = tool
            .execute(
                json!({
                    "description": "inspect code",
                    "prompt": "inspect the workspace",
                    "run_in_background": false,
                    "team_id": "team-demo",
                    "member_id": "review"
                }),
                &ctx,
            )
            .await
            .unwrap();

        let metadata = result.metadata.unwrap();
        let team_state = metadata
            .get("team_state_artifact")
            .and_then(|value| value.as_str())
            .unwrap();
        let team_monitor = metadata
            .get("team_monitor_artifact")
            .and_then(|value| value.as_str())
            .unwrap();
        assert!(std::path::Path::new(team_state).exists());
        assert!(std::path::Path::new(team_monitor).exists());
        let snapshot = ctx
            .team_runtime
            .as_ref()
            .unwrap()
            .lock()
            .await
            .snapshot("team-demo")
            .unwrap();
        assert_eq!(
            snapshot.state.as_ref().unwrap().members[0].status,
            "completed"
        );
    }

    #[tokio::test]
    async fn agent_tool_accepts_team_aliases_and_consumes_mailbox() {
        let dir = tempfile::tempdir().unwrap();
        let prompt = Arc::new(StdMutex::new(None));
        let options = Arc::new(StdMutex::new(None));
        let manager = Arc::new(tokio::sync::Mutex::new(AgentTeamManager::new()));
        {
            let mut manager = manager.lock().await;
            manager.ensure_team(
                "ship feature",
                Some("team-alpha"),
                "manual",
                vec![crate::builtin::team_runtime::AgentTeamMemberState {
                    member_id: "review".to_string(),
                    description: "review".to_string(),
                    subagent_type: None,
                    model: None,
                    run_in_background: false,
                    allowed_tools: vec![],
                    permission_inheritance: "parent_tool_pool".to_string(),
                    status: "planned".to_string(),
                    runtime_task_id: None,
                    last_result_preview: None,
                    result_artifact_path: None,
                    last_updated_at: None,
                    pending_message_count: 0,
                    last_message_at: None,
                }],
            );
            manager
                .append_message("team-alpha", "review", "handoff", "focus on tests")
                .unwrap();
        }

        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());
        ctx.sub_agent_runner = Some(Arc::new(CapturingRunner {
            prompt: Arc::clone(&prompt),
            options: Arc::clone(&options),
        }));
        ctx.team_runtime = Some(Arc::clone(&manager));

        let result = AgentTool
            .execute(
                json!({
                    "description": "inspect code",
                    "prompt": "inspect the workspace",
                    "team_name": "team-alpha",
                    "name": "review"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        let captured_prompt = prompt.lock().unwrap().clone().unwrap();
        assert!(captured_prompt.contains("Team mailbox:"));
        assert!(captured_prompt.contains("focus on tests"));
        let captured_options = options.lock().unwrap().clone().unwrap();
        assert_eq!(captured_options.team_id.as_deref(), Some("team-alpha"));
        assert_eq!(captured_options.member_id.as_deref(), Some("review"));
        let snapshot = manager.lock().await.snapshot("team-alpha").unwrap();
        assert_eq!(
            snapshot.state.as_ref().unwrap().members[0].pending_message_count,
            0
        );
    }
}
