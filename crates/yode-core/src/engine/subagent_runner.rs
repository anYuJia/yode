use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use yode_llm::provider::LlmProvider;
use yode_tools::registry::ToolRegistry;
use yode_tools::runtime_tasks::{latest_transcript_artifact_path, RuntimeTaskStore};
use yode_tools::tool::{SubAgentOptions, SubAgentRunner};

use crate::context::{AgentContext, QuerySource};
use crate::permission::PermissionManager;

use super::{AgentEngine, EngineEvent};

/// Implementation of SubAgentRunner that creates a fresh AgentEngine for each sub-agent.
pub struct SubAgentRunnerImpl {
    pub provider: Arc<dyn LlmProvider>,
    pub tools: Arc<ToolRegistry>,
    pub context: AgentContext,
    pub runtime_tasks: Arc<Mutex<RuntimeTaskStore>>,
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

                let provider = Arc::clone(&self.provider);
                let tools = Arc::clone(&self.tools);
                let mut sub_context = self.context.clone();
                if let Some(m) = subagent_model.clone() {
                    sub_context.model = m;
                }
                let runtime_tasks = Arc::clone(&self.runtime_tasks);
                let turn_prompt = format!("[Sub-task: {}]\n\n{}", options.description, prompt);
                let allowed_tools = allowed_tools.clone();
                let task_id = task.id.clone();
                tokio::spawn(async move {
                    {
                        let mut store = runtime_tasks.lock().await;
                        store.mark_running(&task_id);
                        store.update_progress(
                            &task_id,
                            format!("Running sub-agent task {}", options.description),
                        );
                    }

                    let mut sub_registry = ToolRegistry::new();
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
                            } else {
                                runtime_tasks.lock().await.mark_completed(&task_id);
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
                        }
                    }
                });

                return Ok(format!(
                    "Background sub-agent launched as {}. Output: {}",
                    task.id, output_path_str
                ));
            }

            let mut sub_registry = ToolRegistry::new();
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

            Ok(result_text)
        })
    }
}
