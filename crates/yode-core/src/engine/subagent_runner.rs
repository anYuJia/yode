use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use yode_llm::provider::LlmProvider;
use yode_tools::registry::ToolRegistry;
use yode_tools::runtime_tasks::{latest_transcript_artifact_path, RuntimeTaskStore};
use yode_tools::builtin::team_runtime::update_agent_team_member;
use yode_tools::tool::{SubAgentOptions, SubAgentRunner};

use crate::context::{AgentContext, QuerySource};
use crate::hooks::{HookContext, HookEvent, HookManager};
use crate::permission::PermissionManager;

use super::{AgentEngine, EngineEvent};

/// Implementation of SubAgentRunner that creates a fresh AgentEngine for each sub-agent.
pub struct SubAgentRunnerImpl {
    pub provider: Arc<dyn LlmProvider>,
    pub tools: Arc<ToolRegistry>,
    pub context: AgentContext,
    pub runtime_tasks: Arc<Mutex<RuntimeTaskStore>>,
    pub hook_manager: Option<Arc<HookManager>>,
}

impl SubAgentRunner for SubAgentRunnerImpl {
    fn run_sub_agent(
        &self,
        prompt: String,
        options: SubAgentOptions,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + '_>> {
        let allowed_tools = options.allowed_tools.clone();
        let subagent_model = options.model.clone();

        Box::pin(async move {
            emit_subagent_hook(
                self.hook_manager.as_ref(),
                HookEvent::SubagentStart,
                &self.context,
                &options.description,
                None,
                None,
                Some(serde_json::json!({
                    "run_in_background": options.run_in_background,
                    "isolation": options.isolation,
                    "model_override": options.model,
                    "cwd_override": options.cwd.as_ref().map(|path| path.display().to_string()),
                    "allowed_tools_count": options.allowed_tools.len(),
                })),
            )
            .await;
            if options.run_in_background {
                let tasks_dir = self
                    .context
                    .working_dir_compat()
                    .join(".yode")
                    .join("tasks");
                tokio::fs::create_dir_all(&tasks_dir).await?;
                let output_path = tasks_dir.join(format!("agent-{}.log", uuid::Uuid::new_v4()));
                let output_path_str = output_path.display().to_string();
                let transcript_path =
                    latest_transcript_artifact_path(&self.context.working_dir_compat());
                let (task, mut cancel_rx) = {
                    let mut store = self.runtime_tasks.lock().await;
                    store.create_with_transcript(
                        "agent".to_string(),
                        "agent".to_string(),
                        options.description.clone(),
                        output_path_str.clone(),
                        transcript_path,
                    )
                };
                emit_task_hook(
                    self.hook_manager.as_ref(),
                    HookEvent::TaskCreated,
                    &self.context,
                    &task.id,
                    &options.description,
                    "agent",
                    Some("pending"),
                    None,
                    Some(serde_json::json!({
                        "run_in_background": true,
                        "allowed_tools_count": allowed_tools.len(),
                    })),
                )
                .await;

                let provider = Arc::clone(&self.provider);
                let tools = Arc::clone(&self.tools);
                let mut sub_context = self.context.clone();
                if let Some(m) = subagent_model.clone() {
                    sub_context.model = m;
                }
                let runtime_tasks = Arc::clone(&self.runtime_tasks);
                let hook_manager = self.hook_manager.clone();
                let turn_prompt = format!("[Sub-task: {}]\n\n{}", options.description, prompt);
                let allowed_tools = allowed_tools.clone();
                let task_id = task.id.clone();
                let subagent_description = options.description.clone();
                let team_id = options.team_id.clone();
                let member_id = options.member_id.clone();
                tokio::spawn(async move {
                    {
                        let mut store = runtime_tasks.lock().await;
                        store.mark_running(&task_id);
                        store.update_progress(
                            &task_id,
                            format!("Running sub-agent task {}", options.description),
                        );
                    }

                    let sub_registry = ToolRegistry::new();
                    if allowed_tools.is_empty() {
                        for tool in tools.list() {
                            sub_registry.register(tool);
                        }
                    } else {
                        for name in &allowed_tools {
                            if let Some(tool) = tools.get(name) {
                                sub_registry.register(tool);
                            }
                        }
                    }

                    let permissions = PermissionManager::permissive();
                    let hook_context = sub_context.clone();
                    let mut engine = AgentEngine::new(
                        provider,
                        Arc::new(sub_registry),
                        permissions,
                        sub_context,
                    );
                    let (_confirm_tx, confirm_rx) = mpsc::unbounded_channel();
                    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
                    let cancel_token = CancellationToken::new();
                    let engine_cancel = cancel_token.clone();

                    let engine_handle = tokio::spawn(async move {
                        engine
                            .run_turn_streaming(
                                &turn_prompt,
                                QuerySource::SubAgent,
                                event_tx,
                                confirm_rx,
                                Some(engine_cancel),
                            )
                            .await
                    });

                    let mut result_text = String::new();
                    let mut error_text = None::<String>;
                    let mut cancelled = false;
                    loop {
                        tokio::select! {
                            maybe_event = event_rx.recv() => {
                                match maybe_event {
                                    Some(EngineEvent::Thinking) => {
                                        runtime_tasks.lock().await.update_progress(
                                            &task_id,
                                            "thinking".to_string(),
                                        );
                                    }
                                    Some(EngineEvent::ToolCallStart { name, .. }) => {
                                        runtime_tasks.lock().await.update_progress(
                                            &task_id,
                                            format!("tool: {}", name),
                                        );
                                    }
                                    Some(EngineEvent::ToolProgress { name, progress, .. }) => {
                                        runtime_tasks.lock().await.update_progress(
                                            &task_id,
                                            format!("{}: {}", name, progress.message),
                                        );
                                    }
                                    Some(EngineEvent::ContextCompressed { mode, .. }) => {
                                        runtime_tasks.lock().await.update_progress(
                                            &task_id,
                                            format!("context compacted ({})", mode),
                                        );
                                    }
                                    Some(EngineEvent::TextComplete(text)) => {
                                        result_text = text;
                                    }
                                    Some(EngineEvent::Error(err)) => {
                                        error_text = Some(err);
                                    }
                                    Some(EngineEvent::Done) | None => break,
                                    _ => {}
                                }
                            }
                            changed = cancel_rx.changed() => {
                                if changed.is_ok() && *cancel_rx.borrow() {
                                    cancelled = true;
                                    cancel_token.cancel();
                                    break;
                                }
                            }
                        }
                    }

                    let engine_result = engine_handle.await;
                    if cancelled {
                        let _ = tokio::fs::write(&output_path, "Sub-agent task cancelled.\n").await;
                        runtime_tasks.lock().await.mark_cancelled(&task_id);
                        if let (Some(team_id), Some(member_id)) = (team_id.as_deref(), member_id.as_deref()) {
                            let _ = update_agent_team_member(
                                &hook_context.working_dir_compat(),
                                team_id,
                                member_id,
                                "cancelled",
                                Some(task_id.clone()),
                                Some("Sub-agent task cancelled.".to_string()),
                                None,
                            );
                        }
                        emit_task_hook(
                            hook_manager.as_ref(),
                            HookEvent::TaskCompleted,
                            &hook_context,
                            &task_id,
                            &subagent_description,
                            "agent",
                            Some("cancelled"),
                            Some("Sub-agent task cancelled."),
                            None,
                        )
                        .await;
                        emit_subagent_hook(
                            hook_manager.as_ref(),
                            HookEvent::SubagentStop,
                            &hook_context,
                            &subagent_description,
                            Some("cancelled"),
                            Some("Sub-agent task cancelled."),
                            None,
                        )
                        .await;
                        return;
                    }

                    match engine_result {
                        Ok(Ok(())) => {
                            let content = if result_text.is_empty() {
                                "Sub-agent completed without text output.".to_string()
                            } else {
                                result_text
                            };
                            let _ = tokio::fs::write(&output_path, content).await;
                            if let Some(error) = error_text {
                                runtime_tasks.lock().await.mark_failed(&task_id, error);
                                if let (Some(team_id), Some(member_id)) =
                                    (team_id.as_deref(), member_id.as_deref())
                                {
                                    let _ = update_agent_team_member(
                                        &hook_context.working_dir_compat(),
                                        team_id,
                                        member_id,
                                        "failed",
                                        Some(task_id.clone()),
                                        Some("Sub-agent finished with error text.".to_string()),
                                        None,
                                    );
                                }
                                emit_task_hook(
                                    hook_manager.as_ref(),
                                    HookEvent::TaskCompleted,
                                    &hook_context,
                                    &task_id,
                                    &subagent_description,
                                    "agent",
                                    Some("failed"),
                                    Some("Sub-agent finished with error text."),
                                    None,
                                )
                                .await;
                                emit_subagent_hook(
                                    hook_manager.as_ref(),
                                    HookEvent::SubagentStop,
                                    &hook_context,
                                    &subagent_description,
                                    Some("failed"),
                                    Some("Sub-agent finished with error text."),
                                    None,
                                )
                                .await;
                            } else {
                                runtime_tasks.lock().await.mark_completed(&task_id);
                                if let (Some(team_id), Some(member_id)) =
                                    (team_id.as_deref(), member_id.as_deref())
                                {
                                    let _ = update_agent_team_member(
                                        &hook_context.working_dir_compat(),
                                        team_id,
                                        member_id,
                                        "completed",
                                        Some(task_id.clone()),
                                        Some("Sub-agent completed successfully.".to_string()),
                                        None,
                                    );
                                }
                                emit_task_hook(
                                    hook_manager.as_ref(),
                                    HookEvent::TaskCompleted,
                                    &hook_context,
                                    &task_id,
                                    &subagent_description,
                                    "agent",
                                    Some("completed"),
                                    Some("Sub-agent completed successfully."),
                                    None,
                                )
                                .await;
                                emit_subagent_hook(
                                    hook_manager.as_ref(),
                                    HookEvent::SubagentStop,
                                    &hook_context,
                                    &subagent_description,
                                    Some("completed"),
                                    Some("Sub-agent completed successfully."),
                                    None,
                                )
                                .await;
                            }
                        }
                        Ok(Err(err)) => {
                            let _ = tokio::fs::write(
                                &output_path,
                                format!("Sub-agent failed: {}", err),
                            )
                            .await;
                            runtime_tasks
                                .lock()
                                .await
                                .mark_failed(&task_id, format!("{}", err));
                            if let (Some(team_id), Some(member_id)) =
                                (team_id.as_deref(), member_id.as_deref())
                            {
                                let _ = update_agent_team_member(
                                    &hook_context.working_dir_compat(),
                                    team_id,
                                    member_id,
                                    "failed",
                                    Some(task_id.clone()),
                                    Some(format!("{}", err)),
                                    None,
                                );
                            }
                            emit_task_hook(
                                hook_manager.as_ref(),
                                HookEvent::TaskCompleted,
                                &hook_context,
                                &task_id,
                                &subagent_description,
                                "agent",
                                Some("failed"),
                                Some(&format!("{}", err)),
                                None,
                            )
                            .await;
                            emit_subagent_hook(
                                hook_manager.as_ref(),
                                HookEvent::SubagentStop,
                                &hook_context,
                                &subagent_description,
                                Some("failed"),
                                Some(&format!("{}", err)),
                                None,
                            )
                            .await;
                        }
                        Err(err) => {
                            let _ = tokio::fs::write(
                                &output_path,
                                format!("Sub-agent task join failure: {}", err),
                            )
                            .await;
                            runtime_tasks
                                .lock()
                                .await
                                .mark_failed(&task_id, format!("Join error: {}", err));
                            if let (Some(team_id), Some(member_id)) =
                                (team_id.as_deref(), member_id.as_deref())
                            {
                                let _ = update_agent_team_member(
                                    &hook_context.working_dir_compat(),
                                    team_id,
                                    member_id,
                                    "failed",
                                    Some(task_id.clone()),
                                    Some(format!("Join error: {}", err)),
                                    None,
                                );
                            }
                            emit_task_hook(
                                hook_manager.as_ref(),
                                HookEvent::TaskCompleted,
                                &hook_context,
                                &task_id,
                                &subagent_description,
                                "agent",
                                Some("failed"),
                                Some(&format!("Join error: {}", err)),
                                None,
                            )
                            .await;
                            emit_subagent_hook(
                                hook_manager.as_ref(),
                                HookEvent::SubagentStop,
                                &hook_context,
                                &subagent_description,
                                Some("failed"),
                                Some(&format!("Join error: {}", err)),
                                None,
                            )
                            .await;
                        }
                    }
                });

                return Ok(format!(
                    "Background sub-agent launched as {}. Output: {}",
                    task.id, output_path_str
                ));
            }

            let sub_registry = ToolRegistry::new();
            if allowed_tools.is_empty() {
                for tool in self.tools.list() {
                    sub_registry.register(tool);
                }
            } else {
                for name in &allowed_tools {
                    if let Some(tool) = self.tools.get(name) {
                        sub_registry.register(tool);
                    }
                }
            }

            let sub_registry = Arc::new(sub_registry);
            let permissions = PermissionManager::permissive();

            let mut sub_context = self.context.clone();
            if let Some(m) = subagent_model {
                sub_context.model = m;
            }

            let mut engine = AgentEngine::new(
                Arc::clone(&self.provider),
                sub_registry,
                permissions,
                sub_context,
            );

            let (_confirm_tx, confirm_rx) = mpsc::unbounded_channel();
            let turn_prompt = format!("[Sub-task: {}]\n\n{}", options.description, prompt);
            let (result_tx, mut result_rx) = mpsc::unbounded_channel();

            let engine_handle = tokio::spawn(async move {
                engine
                    .run_turn(&turn_prompt, QuerySource::SubAgent, result_tx, confirm_rx)
                    .await
            });

            let mut result_text = String::new();
            while let Some(event) = result_rx.recv().await {
                if let EngineEvent::TextComplete(text) = event {
                    result_text = text;
                }
            }

            engine_handle.await??;

            if result_text.is_empty() {
                result_text = "Sub-agent completed without text output.".to_string();
            }

            emit_subagent_hook(
                self.hook_manager.as_ref(),
                HookEvent::SubagentStop,
                &self.context,
                &options.description,
                Some("completed"),
                Some("Foreground sub-agent completed."),
                Some(serde_json::json!({
                    "run_in_background": false,
                    "isolation": options.isolation,
                    "model_override": options.model,
                    "cwd_override": options.cwd.as_ref().map(|path| path.display().to_string()),
                    "allowed_tools_count": options.allowed_tools.len(),
                })),
            )
            .await;

            Ok(result_text)
        })
    }
}

async fn emit_subagent_hook(
    hook_manager: Option<&Arc<HookManager>>,
    event: HookEvent,
    context: &AgentContext,
    description: &str,
    status: Option<&str>,
    summary: Option<&str>,
    extra_metadata: Option<serde_json::Value>,
) {
    let Some(hook_manager) = hook_manager else {
        return;
    };
    let hook_context = HookContext {
        event: event.to_string(),
        session_id: context.session_id.clone(),
        working_dir: context.working_dir_compat().display().to_string(),
        tool_name: Some("agent".to_string()),
        tool_input: None,
        tool_output: summary.map(str::to_string),
        error: status
            .filter(|status| *status == "failed" || *status == "cancelled")
            .and(summary.map(str::to_string)),
        user_prompt: None,
        metadata: Some(merge_hook_metadata(
            serde_json::json!({
            "description": description,
            "status": status,
            }),
            extra_metadata,
        )),
    };
    let _ = hook_manager.execute(event, &hook_context).await;
}

async fn emit_task_hook(
    hook_manager: Option<&Arc<HookManager>>,
    event: HookEvent,
    context: &AgentContext,
    task_id: &str,
    description: &str,
    kind: &str,
    status: Option<&str>,
    error: Option<&str>,
    extra_metadata: Option<serde_json::Value>,
) {
    let Some(hook_manager) = hook_manager else {
        return;
    };
    let hook_context = HookContext {
        event: event.to_string(),
        session_id: context.session_id.clone(),
        working_dir: context.working_dir_compat().display().to_string(),
        tool_name: Some("runtime_task".to_string()),
        tool_input: None,
        tool_output: status.map(str::to_string),
        error: error.map(str::to_string),
        user_prompt: None,
        metadata: Some(merge_hook_metadata(
            serde_json::json!({
            "task_id": task_id,
            "description": description,
            "kind": kind,
            "status": status,
            }),
            extra_metadata,
        )),
    };
    let _ = hook_manager.execute(event, &hook_context).await;
}

fn merge_hook_metadata(
    base: serde_json::Value,
    extra: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut base_object = base.as_object().cloned().unwrap_or_default();
    if let Some(extra) = extra.and_then(|value| value.as_object().cloned()) {
        for (key, value) in extra {
            base_object.insert(key, value);
        }
    }
    serde_json::Value::Object(base_object)
}

#[cfg(test)]
mod tests {
    use super::{emit_subagent_hook, emit_task_hook};
    use crate::context::AgentContext;
    use crate::hooks::{HookEvent, HookManager};

    #[tokio::test]
    async fn subagent_hook_emits_rich_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let dump_path = dir.path().join("subagent-start.json");
        let mut hook_mgr = HookManager::new(dir.path().to_path_buf());
        hook_mgr.register(crate::hooks::HookDefinition {
            command: format!(
                "printf '%s' \"$YODE_HOOK_CONTEXT\" > {}",
                dump_path.display()
            ),
            events: vec!["subagent_start".into()],
            tool_filter: Some(vec!["agent".into()]),
            timeout_secs: 5,
            can_block: false,
        });
        let context = AgentContext::new(
            dir.path().to_path_buf(),
            "mock".to_string(),
            "claude-sonnet-4".to_string(),
        );

        emit_subagent_hook(
            Some(&std::sync::Arc::new(hook_mgr)),
            HookEvent::SubagentStart,
            &context,
            "analyze hook parity",
            Some("running"),
            Some("subagent started"),
            Some(serde_json::json!({
                "run_in_background": true,
                "allowed_tools_count": 3,
            })),
        )
        .await;

        let body = std::fs::read_to_string(dump_path).unwrap();
        assert!(body.contains("\"event\":\"subagent_start\""));
        assert!(body.contains("\"description\":\"analyze hook parity\""));
        assert!(body.contains("\"allowed_tools_count\":3"));
    }

    #[tokio::test]
    async fn task_hook_emits_task_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let dump_path = dir.path().join("task-created.json");
        let mut hook_mgr = HookManager::new(dir.path().to_path_buf());
        hook_mgr.register(crate::hooks::HookDefinition {
            command: format!(
                "printf '%s' \"$YODE_HOOK_CONTEXT\" > {}",
                dump_path.display()
            ),
            events: vec!["task_created".into()],
            tool_filter: Some(vec!["runtime_task".into()]),
            timeout_secs: 5,
            can_block: false,
        });
        let context = AgentContext::new(
            dir.path().to_path_buf(),
            "mock".to_string(),
            "claude-sonnet-4".to_string(),
        );

        emit_task_hook(
            Some(&std::sync::Arc::new(hook_mgr)),
            HookEvent::TaskCreated,
            &context,
            "task-1",
            "background agent",
            "agent",
            Some("pending"),
            None,
            Some(serde_json::json!({
                "run_in_background": true,
            })),
        )
        .await;

        let body = std::fs::read_to_string(dump_path).unwrap();
        assert!(body.contains("\"event\":\"task_created\""));
        assert!(body.contains("\"task_id\":\"task-1\""));
        assert!(body.contains("\"run_in_background\":true"));
    }
}
