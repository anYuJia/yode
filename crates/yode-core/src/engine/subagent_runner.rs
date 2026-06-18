use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use yode_agent::AgentTeamManager;
use yode_llm::provider::LlmProvider;
use yode_tools::builtin::skill::SkillInvocation;
use yode_tools::builtin::team_runtime::{
    hydrate_agent_team_manager, persist_agent_team_snapshot, update_agent_team_member,
};
use yode_tools::registry::ToolRegistry;
use yode_tools::runtime_tasks::{latest_transcript_artifact_path, RuntimeTaskStore};
use yode_tools::tool::{SubAgentOptions, SubAgentRunner};

use crate::context::{AgentContext, QuerySource, SessionRuntime};
use crate::hooks::{HookContext, HookEvent, HookManager};
use crate::permission::PermissionManager;

use super::{AgentEngine, EngineEvent};

const FORK_BOILERPLATE_TAG: &str = "forked-worker";
const FORK_DIRECTIVE_PREFIX: &str = "Directive:";

/// Implementation of SubAgentRunner that creates a fresh AgentEngine for each sub-agent.
pub struct SubAgentRunnerImpl {
    pub provider: Arc<dyn LlmProvider>,
    pub tools: Arc<ToolRegistry>,
    pub context: AgentContext,
    pub runtime_tasks: Arc<Mutex<RuntimeTaskStore>>,
    pub team_runtime: Arc<Mutex<AgentTeamManager>>,
    pub skill_invocation_store: Arc<Mutex<Vec<SkillInvocation>>>,
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
            let parent_working_dir = self.context.working_dir_compat();
            let subagent_context = prepare_subagent_context(&self.context, &options)?;
            let prompt = inject_team_runtime_context(
                prompt,
                &self.team_runtime,
                &parent_working_dir,
                options.team_id.as_deref(),
                options.member_id.as_deref(),
            )
            .await;
            let mut prompt = inject_workspace_context(
                prompt,
                &subagent_context,
                options.isolation.as_deref(),
                options.cwd.as_ref(),
            );
            let fork_prompt_fingerprint = if options.fork_context {
                if prompt.contains(&format!("<{}>", FORK_BOILERPLATE_TAG)) {
                    return Err(anyhow::anyhow!(
                        "Recursive fork_context sub-agents are blocked; execute directly in this fork worker."
                    ));
                }
                prompt = build_fork_child_prompt(
                    &prompt,
                    &parent_working_dir,
                    &subagent_context.working_dir_compat(),
                    options.isolation.as_deref(),
                );
                Some(stable_prompt_fingerprint(&prompt))
            } else {
                None
            };
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
                    "effective_cwd": subagent_context.working_dir_compat().display().to_string(),
                    "allowed_tools_count": options.allowed_tools.len(),
                    "fork_context": options.fork_context,
                    "fork_prompt_fingerprint": fork_prompt_fingerprint.clone(),
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
                    latest_transcript_artifact_path(&self.context.working_dir_compat()).await;
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
                let mut sub_context = subagent_context.clone();
                if let Some(m) = subagent_model.clone() {
                    sub_context.model = m;
                }
                let runtime_tasks = Arc::clone(&self.runtime_tasks);
                let team_runtime = Arc::clone(&self.team_runtime);
                let skill_invocation_store = Arc::clone(&self.skill_invocation_store);
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
                    register_subagent_tools(
                        &sub_registry,
                        &tools,
                        &allowed_tools,
                        team_id.is_some() && member_id.is_some(),
                    );

                    let permissions = PermissionManager::permissive();
                    let hook_context = sub_context.clone();
                    let mut engine = AgentEngine::new(
                        provider,
                        Arc::new(sub_registry),
                        permissions,
                        sub_context,
                    );
                    engine.skill_invocation_store = Arc::clone(&skill_invocation_store);
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
                                    Some(EngineEvent::ContextCompactionStarted { mode }) => {
                                        runtime_tasks.lock().await.update_progress(
                                            &task_id,
                                            format!("compacting context ({})", mode),
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
                        complete_background_subagent(BackgroundSubagentCompletion {
                            runtime_tasks,
                            team_runtime,
                            hook_manager,
                            hook_context,
                            parent_working_dir,
                            task_id,
                            description: subagent_description,
                            output_path,
                            team_id,
                            member_id,
                            status: BackgroundSubagentStatus::Cancelled,
                            output: "Sub-agent task cancelled.\n".to_string(),
                            task_error: None,
                            team_summary: "Sub-agent task cancelled.".to_string(),
                            hook_summary: "Sub-agent task cancelled.".to_string(),
                        })
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
                            if let Some(error) = error_text {
                                complete_background_subagent(BackgroundSubagentCompletion {
                                    runtime_tasks,
                                    team_runtime,
                                    hook_manager,
                                    hook_context,
                                    parent_working_dir,
                                    task_id,
                                    description: subagent_description,
                                    output_path,
                                    team_id,
                                    member_id,
                                    status: BackgroundSubagentStatus::Failed,
                                    output: content,
                                    task_error: Some(error),
                                    team_summary: "Sub-agent finished with error text.".to_string(),
                                    hook_summary: "Sub-agent finished with error text.".to_string(),
                                })
                                .await;
                            } else {
                                complete_background_subagent(BackgroundSubagentCompletion {
                                    runtime_tasks,
                                    team_runtime,
                                    hook_manager,
                                    hook_context,
                                    parent_working_dir,
                                    task_id,
                                    description: subagent_description,
                                    output_path,
                                    team_id,
                                    member_id,
                                    status: BackgroundSubagentStatus::Completed,
                                    output: content,
                                    task_error: None,
                                    team_summary: "Sub-agent completed successfully.".to_string(),
                                    hook_summary: "Sub-agent completed successfully.".to_string(),
                                })
                                .await;
                            }
                        }
                        Ok(Err(err)) => {
                            let error = format!("{}", err);
                            complete_background_subagent(BackgroundSubagentCompletion {
                                runtime_tasks,
                                team_runtime,
                                hook_manager,
                                hook_context,
                                parent_working_dir,
                                task_id,
                                description: subagent_description,
                                output_path,
                                team_id,
                                member_id,
                                status: BackgroundSubagentStatus::Failed,
                                output: format!("Sub-agent failed: {}", error),
                                task_error: Some(error.clone()),
                                team_summary: error.clone(),
                                hook_summary: error,
                            })
                            .await;
                        }
                        Err(err) => {
                            let error = format!("Join error: {}", err);
                            complete_background_subagent(BackgroundSubagentCompletion {
                                runtime_tasks,
                                team_runtime,
                                hook_manager,
                                hook_context,
                                parent_working_dir,
                                task_id,
                                description: subagent_description,
                                output_path,
                                team_id,
                                member_id,
                                status: BackgroundSubagentStatus::Failed,
                                output: format!("Sub-agent task join failure: {}", err),
                                task_error: Some(error.clone()),
                                team_summary: error.clone(),
                                hook_summary: error,
                            })
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
            register_subagent_tools(
                &sub_registry,
                &self.tools,
                &allowed_tools,
                options.team_id.is_some() && options.member_id.is_some(),
            );

            let sub_registry = Arc::new(sub_registry);
            let permissions = PermissionManager::permissive();

            let mut sub_context = subagent_context.clone();
            if let Some(m) = subagent_model {
                sub_context.model = m;
            }

            let mut engine = AgentEngine::new(
                Arc::clone(&self.provider),
                sub_registry,
                permissions,
                sub_context,
            );
            engine.skill_invocation_store = Arc::clone(&self.skill_invocation_store);

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

            sync_team_runtime_update(
                &self.team_runtime,
                &parent_working_dir,
                options.team_id.as_deref(),
                options.member_id.as_deref(),
                "completed",
                None,
                Some(result_text.clone()),
                None,
            )
            .await;

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
                    "fork_context": options.fork_context,
                    "fork_prompt_fingerprint": fork_prompt_fingerprint.clone(),
                })),
            )
            .await;

            Ok(result_text)
        })
    }
}

enum BackgroundSubagentStatus {
    Completed,
    Failed,
    Cancelled,
}

impl BackgroundSubagentStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

struct BackgroundSubagentCompletion {
    runtime_tasks: Arc<Mutex<RuntimeTaskStore>>,
    team_runtime: Arc<Mutex<AgentTeamManager>>,
    hook_manager: Option<Arc<HookManager>>,
    hook_context: AgentContext,
    parent_working_dir: PathBuf,
    task_id: String,
    description: String,
    output_path: PathBuf,
    team_id: Option<String>,
    member_id: Option<String>,
    status: BackgroundSubagentStatus,
    output: String,
    task_error: Option<String>,
    team_summary: String,
    hook_summary: String,
}

async fn complete_background_subagent(completion: BackgroundSubagentCompletion) {
    let _ = tokio::fs::write(&completion.output_path, &completion.output).await;
    {
        let mut store = completion.runtime_tasks.lock().await;
        match completion.status {
            BackgroundSubagentStatus::Completed => store.mark_completed(&completion.task_id),
            BackgroundSubagentStatus::Failed => store.mark_failed(
                &completion.task_id,
                completion
                    .task_error
                    .clone()
                    .unwrap_or_else(|| completion.hook_summary.clone()),
            ),
            BackgroundSubagentStatus::Cancelled => store.mark_cancelled(&completion.task_id),
        }
    }

    let status = completion.status.as_str();
    sync_team_runtime_update(
        &completion.team_runtime,
        &completion.parent_working_dir,
        completion.team_id.as_deref(),
        completion.member_id.as_deref(),
        status,
        Some(completion.task_id.clone()),
        Some(completion.team_summary.clone()),
        None,
    )
    .await;
    emit_task_hook(
        completion.hook_manager.as_ref(),
        HookEvent::TaskCompleted,
        &completion.hook_context,
        &completion.task_id,
        &completion.description,
        "agent",
        Some(status),
        Some(&completion.hook_summary),
        None,
    )
    .await;
    emit_subagent_hook(
        completion.hook_manager.as_ref(),
        HookEvent::SubagentStop,
        &completion.hook_context,
        &completion.description,
        Some(status),
        Some(&completion.hook_summary),
        None,
    )
    .await;
}

#[expect(
    clippy::too_many_arguments,
    reason = "team runtime sync writes a complete member status/result tuple to persisted state"
)]
async fn sync_team_runtime_update(
    manager: &Arc<Mutex<AgentTeamManager>>,
    working_dir: &std::path::Path,
    team_id: Option<&str>,
    member_id: Option<&str>,
    status: &str,
    runtime_task_id: Option<String>,
    result_preview: Option<String>,
    result_artifact_path: Option<String>,
) {
    let (Some(team_id), Some(member_id)) = (team_id, member_id) else {
        return;
    };

    let snapshot = {
        let mut manager = manager.lock().await;
        let _ = hydrate_agent_team_manager(working_dir, &mut manager, team_id);
        let _ = manager.update_member(
            team_id,
            member_id,
            status,
            runtime_task_id.clone(),
            result_preview.clone(),
            result_artifact_path.clone(),
        );
        manager.snapshot(team_id)
    };

    if let Some(snapshot) = snapshot.as_ref() {
        let _ = persist_agent_team_snapshot(working_dir, snapshot);
    } else {
        let _ = update_agent_team_member(
            working_dir,
            team_id,
            member_id,
            status,
            runtime_task_id,
            result_preview,
            result_artifact_path,
        );
    }
}

async fn inject_team_runtime_context(
    prompt: String,
    manager: &Arc<Mutex<AgentTeamManager>>,
    working_dir: &std::path::Path,
    team_id: Option<&str>,
    member_id: Option<&str>,
) -> String {
    let (Some(team_id), Some(member_id)) = (team_id, member_id) else {
        return prompt;
    };
    let messages = {
        let mut manager = manager.lock().await;
        let _ = hydrate_agent_team_manager(working_dir, &mut manager, team_id);
        let messages = manager.consume_message_context(team_id, member_id, 8);
        if !messages.is_empty() {
            if let Some(snapshot) = manager.snapshot(team_id) {
                let _ = persist_agent_team_snapshot(working_dir, &snapshot);
            }
        }
        messages
    };
    let mailbox = if messages.is_empty() {
        "No pending messages at launch.".to_string()
    } else {
        messages
            .iter()
            .map(|message| {
                format!(
                    "{} [{}:{}] {}",
                    message.at, message.target, message.kind, message.message
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "Team runtime:\n- team_id: {}\n- member_id: {}\n- Use `team_receive` with this team_id/member_id to check messages sent while you are running.\n\nTeam mailbox:\n{}\n\nTask:\n{}",
        team_id, member_id, mailbox, prompt
    )
}

fn prepare_subagent_context(
    context: &AgentContext,
    options: &SubAgentOptions,
) -> Result<AgentContext> {
    let mut sub_context = context.clone();
    sub_context.subagent_description = Some(options.description.clone());
    sub_context.subagent_type = options.subagent_type.clone();
    sub_context.team_id = options.team_id.clone();
    sub_context.member_id = options.member_id.clone();
    let target_cwd = if let Some(cwd) = options.cwd.as_ref() {
        Some(cwd.clone())
    } else if options.isolation.as_deref() == Some("worktree") {
        Some(create_agent_worktree(
            &context.working_dir_compat(),
            &options.description,
        )?)
    } else if let Some(isolation) = options.isolation.as_deref() {
        return Err(anyhow::anyhow!(
            "Unsupported sub-agent isolation mode '{}'.",
            isolation
        ));
    } else {
        None
    };

    if let Some(cwd) = target_cwd {
        if !cwd.is_dir() {
            return Err(anyhow::anyhow!(
                "Sub-agent cwd '{}' is not a directory.",
                cwd.display()
            ));
        }
        sub_context.project_root = cwd.clone();
        sub_context.runtime = Arc::new(Mutex::new(SessionRuntime::new(cwd)));
    }
    Ok(sub_context)
}

fn inject_workspace_context(
    prompt: String,
    context: &AgentContext,
    isolation: Option<&str>,
    cwd_override: Option<&std::path::PathBuf>,
) -> String {
    if isolation.is_none() && cwd_override.is_none() {
        return prompt;
    }
    format!(
        "Sub-agent workspace:\n- cwd: {}\n- isolation: {}\n\nTask:\n{}",
        context.working_dir_compat().display(),
        isolation.unwrap_or("cwd_override"),
        prompt
    )
}

fn build_fork_child_prompt(
    directive: &str,
    parent_cwd: &std::path::Path,
    child_cwd: &std::path::Path,
    isolation: Option<&str>,
) -> String {
    let worktree_notice = (isolation == Some("worktree")).then(|| {
        build_worktree_notice(
            &parent_cwd.display().to_string(),
            &child_cwd.display().to_string(),
        )
    });
    format!(
        "<{tag}>\nSTOP. READ THIS FIRST.\n\nYou are a forked worker process. You are NOT the main agent.\n\nRULES:\n1. Do NOT spawn sub-agents or fork workers; execute directly.\n2. Use tools directly and stay within the directive's scope.\n3. Re-read files before editing when inherited context may be stale.\n4. Report concise structured facts, then stop.\n{notice}</{tag}>\n\n{prefix} {directive}",
        tag = FORK_BOILERPLATE_TAG,
        notice = worktree_notice
            .map(|notice| format!("\nWorktree notice:\n{}\n", notice))
            .unwrap_or_default(),
        prefix = FORK_DIRECTIVE_PREFIX,
    )
}

fn build_worktree_notice(parent_cwd: &str, worktree_cwd: &str) -> String {
    format!(
        "You've inherited context from a parent agent working in {parent_cwd}. You are operating in an isolated git worktree at {worktree_cwd}; translate inherited paths to this worktree root and re-read files before editing. Changes stay isolated from the parent's working copy."
    )
}

fn stable_prompt_fingerprint(prompt: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in prompt.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn create_agent_worktree(root: &std::path::Path, description: &str) -> Result<std::path::PathBuf> {
    let git_root_output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(root)
        .output()
        .map_err(|err| anyhow::anyhow!("Failed to locate git root: {}", err))?;
    if !git_root_output.status.success() {
        return Err(anyhow::anyhow!(
            "Sub-agent worktree isolation requires a git repository."
        ));
    }
    let git_root = std::path::PathBuf::from(
        String::from_utf8_lossy(&git_root_output.stdout)
            .trim()
            .to_string(),
    );
    let suffix = uuid::Uuid::new_v4().to_string();
    let short_suffix = &suffix[..8];
    let slug = sanitize_workspace_name(description);
    let name = if slug.is_empty() {
        format!("agent-{}", short_suffix)
    } else {
        format!("agent-{}-{}", slug, short_suffix)
    };
    let branch = format!("yode-{}", name);
    let worktree_dir = git_root.join(".yode").join("agent-worktrees").join(&name);
    std::fs::create_dir_all(
        worktree_dir
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid worktree path."))?,
    )?;

    let output = std::process::Command::new("git")
        .args([
            "worktree",
            "add",
            &worktree_dir.display().to_string(),
            "-b",
            &branch,
            "HEAD",
        ])
        .current_dir(&git_root)
        .output()
        .map_err(|err| anyhow::anyhow!("Failed to create sub-agent worktree: {}", err))?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "git worktree add failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(worktree_dir)
}

fn sanitize_workspace_name(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .chars()
        .take(32)
        .collect()
}

fn register_subagent_tools(
    sub_registry: &ToolRegistry,
    tools: &ToolRegistry,
    allowed_tools: &[String],
    include_team_runtime_tools: bool,
) {
    if allowed_tools.is_empty() {
        for tool in tools.list() {
            sub_registry.register(tool);
        }
        return;
    }

    for name in allowed_tools {
        if let Some(tool) = tools.get(name) {
            sub_registry.register(tool);
        }
    }
    if include_team_runtime_tools {
        for name in ["team_receive", "send_message", "team_monitor"] {
            if allowed_tools.iter().any(|allowed| allowed == name) {
                continue;
            }
            if let Some(tool) = tools.get(name) {
                sub_registry.register(tool);
            }
        }
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
    let hook_context = HookContext::new(
        event.clone(),
        context.session_id.clone(),
        context.working_dir_compat().display().to_string(),
    )
    .with_tool("agent", None)
    .with_tool_output(summary.map(str::to_string))
    .with_error(
        status
            .filter(|status| *status == "failed" || *status == "cancelled")
            .and(summary.map(str::to_string)),
    )
    .with_metadata(Some(merge_hook_metadata(
        serde_json::json!({
        "description": description,
        "status": status,
        }),
        extra_metadata,
    )));
    let _ = hook_manager.execute(event, &hook_context).await;
}

#[expect(
    clippy::too_many_arguments,
    reason = "task hook emission mirrors hook protocol fields and metadata in one boundary"
)]
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
    let hook_context = HookContext::new(
        event.clone(),
        context.session_id.clone(),
        context.working_dir_compat().display().to_string(),
    )
    .with_tool("runtime_task", None)
    .with_tool_output(status.map(str::to_string))
    .with_error(error.map(str::to_string))
    .with_metadata(Some(merge_hook_metadata(
        serde_json::json!({
        "task_id": task_id,
        "description": description,
        "kind": kind,
        "status": status,
        }),
        extra_metadata,
    )));
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
    use super::{
        build_fork_child_prompt, complete_background_subagent, emit_subagent_hook, emit_task_hook,
        inject_workspace_context, prepare_subagent_context, sanitize_workspace_name,
        stable_prompt_fingerprint, BackgroundSubagentCompletion, BackgroundSubagentStatus,
        FORK_BOILERPLATE_TAG, FORK_DIRECTIVE_PREFIX,
    };
    use crate::context::AgentContext;
    use crate::hooks::{HookEvent, HookManager};
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use yode_agent::{AgentTeamManager, AgentTeamMemberState};
    use yode_tools::runtime_tasks::{RuntimeTaskStatus, RuntimeTaskStore};
    use yode_tools::tool::SubAgentOptions;

    fn hook_dump_context_command(path: &std::path::Path) -> String {
        #[cfg(windows)]
        {
            return crate::test_support::powershell_encoded_command(&format!(
                "[System.IO.File]::WriteAllText('{}', $env:YODE_HOOK_CONTEXT)",
                powershell_quote_path(path)
            ));
        }

        #[cfg(not(windows))]
        format!(
            "printf '%s' \"$YODE_HOOK_CONTEXT\" > {}",
            shell_quote_path(path)
        )
    }

    #[cfg(windows)]
    fn powershell_quote_path(path: &std::path::Path) -> String {
        path.display().to_string().replace('\'', "''")
    }

    #[cfg(not(windows))]
    fn shell_quote_path(path: &std::path::Path) -> String {
        let rendered = path.display().to_string();

        format!("'{}'", rendered.replace('\'', "'\\''"))
    }

    #[tokio::test]
    async fn subagent_hook_emits_rich_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let dump_path = dir.path().join("subagent-start.json");
        let mut hook_mgr = HookManager::new(dir.path().to_path_buf());
        hook_mgr.register(crate::hooks::HookDefinition {
            command: hook_dump_context_command(&dump_path),
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
            command: hook_dump_context_command(&dump_path),
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

    #[tokio::test]
    async fn background_subagent_completion_updates_task_and_team() {
        let dir = tempfile::tempdir().unwrap();
        let output_path = dir.path().join("agent.log");
        let runtime_tasks = Arc::new(Mutex::new(RuntimeTaskStore::new()));
        let (task, _cancel_rx) = runtime_tasks.lock().await.create(
            "agent".to_string(),
            "agent".to_string(),
            "implement worker".to_string(),
            output_path.display().to_string(),
        );

        let mut team = AgentTeamManager::new();
        team.ensure_team(
            "ship feature",
            Some("team-1"),
            "parallel",
            vec![AgentTeamMemberState {
                member_id: "worker-1".to_string(),
                description: "implement worker".to_string(),
                subagent_type: None,
                model: None,
                run_in_background: true,
                allowed_tools: Vec::new(),
                permission_inheritance: "permissive".to_string(),
                status: "running".to_string(),
                runtime_task_id: None,
                last_result_preview: None,
                result_artifact_path: None,
                last_updated_at: None,
                pending_message_count: 0,
                last_message_at: None,
            }],
        );
        let team_runtime = Arc::new(Mutex::new(team));
        let context = AgentContext::new(
            dir.path().to_path_buf(),
            "mock".to_string(),
            "claude-sonnet-4".to_string(),
        );

        complete_background_subagent(BackgroundSubagentCompletion {
            runtime_tasks: Arc::clone(&runtime_tasks),
            team_runtime: Arc::clone(&team_runtime),
            hook_manager: None,
            hook_context: context,
            parent_working_dir: dir.path().to_path_buf(),
            task_id: task.id.clone(),
            description: "implement worker".to_string(),
            output_path: output_path.clone(),
            team_id: Some("team-1".to_string()),
            member_id: Some("worker-1".to_string()),
            status: BackgroundSubagentStatus::Completed,
            output: "done".to_string(),
            task_error: None,
            team_summary: "Sub-agent completed successfully.".to_string(),
            hook_summary: "Sub-agent completed successfully.".to_string(),
        })
        .await;

        assert_eq!(
            runtime_tasks.lock().await.get(&task.id).unwrap().status,
            RuntimeTaskStatus::Completed
        );
        assert_eq!(std::fs::read_to_string(output_path).unwrap(), "done");
        let snapshot = team_runtime.lock().await.snapshot("team-1").unwrap();
        let state = snapshot.state.unwrap();
        let member = state
            .members
            .iter()
            .find(|member| member.member_id == "worker-1")
            .unwrap();
        assert_eq!(member.status, "completed");
        assert_eq!(member.runtime_task_id.as_deref(), Some(task.id.as_str()));
    }

    #[test]
    fn subagent_context_applies_cwd_override() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("child");
        std::fs::create_dir_all(&child).unwrap();
        let context = AgentContext::new(
            dir.path().to_path_buf(),
            "mock".to_string(),
            "claude-sonnet-4".to_string(),
        );
        let options = SubAgentOptions {
            description: "inspect child".to_string(),
            cwd: Some(child.clone()),
            ..Default::default()
        };

        let sub_context = prepare_subagent_context(&context, &options).unwrap();

        assert_eq!(sub_context.working_dir_compat(), child);
        assert_eq!(context.working_dir_compat(), dir.path());
    }

    #[test]
    fn subagent_context_carries_invocation_scope() {
        let dir = tempfile::tempdir().unwrap();
        let context = AgentContext::new(
            dir.path().to_path_buf(),
            "mock".to_string(),
            "claude-sonnet-4".to_string(),
        );
        let options = SubAgentOptions {
            description: "review code".to_string(),
            subagent_type: Some("worker".to_string()),
            team_id: Some("team-1".to_string()),
            member_id: Some("reviewer".to_string()),
            ..Default::default()
        };

        let sub_context = prepare_subagent_context(&context, &options).unwrap();

        assert_eq!(
            sub_context.subagent_description.as_deref(),
            Some("review code")
        );
        assert_eq!(sub_context.subagent_type.as_deref(), Some("worker"));
        assert_eq!(sub_context.team_id.as_deref(), Some("team-1"));
        assert_eq!(sub_context.member_id.as_deref(), Some("reviewer"));
    }

    #[test]
    fn workspace_prompt_mentions_effective_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let context = AgentContext::new(
            dir.path().to_path_buf(),
            "mock".to_string(),
            "claude-sonnet-4".to_string(),
        );
        let prompt = inject_workspace_context(
            "do work".to_string(),
            &context,
            None,
            Some(&dir.path().to_path_buf()),
        );

        assert!(prompt.contains("Sub-agent workspace:"));
        assert!(prompt.contains(&dir.path().display().to_string()));
        assert!(prompt.contains("do work"));
    }

    #[test]
    fn fork_child_prompts_share_stable_prefix() {
        let parent = std::path::Path::new("/tmp/parent");
        let child = std::path::Path::new("/tmp/parent");
        let first = build_fork_child_prompt("inspect auth", parent, child, None);
        let second = build_fork_child_prompt("inspect billing", parent, child, None);
        let first_prefix = first.split(FORK_DIRECTIVE_PREFIX).next().unwrap();
        let second_prefix = second.split(FORK_DIRECTIVE_PREFIX).next().unwrap();

        assert_eq!(first_prefix, second_prefix);
        assert!(first_prefix.contains(&format!("<{}>", FORK_BOILERPLATE_TAG)));
        assert_ne!(
            stable_prompt_fingerprint(&first),
            stable_prompt_fingerprint(&second)
        );
    }

    #[test]
    fn fork_child_worktree_prompt_includes_path_translation_notice() {
        let prompt = build_fork_child_prompt(
            "edit docs",
            std::path::Path::new("/repo/main"),
            std::path::Path::new("/repo/worktree"),
            Some("worktree"),
        );

        assert!(prompt.contains("isolated git worktree"));
        assert!(prompt.contains("/repo/main"));
        assert!(prompt.contains("/repo/worktree"));
        assert!(prompt.contains("translate inherited paths"));
    }

    #[test]
    fn workspace_name_sanitizer_keeps_branch_safe_slug() {
        assert_eq!(
            sanitize_workspace_name("Review API & Tests!"),
            "review-api---tests"
        );
    }
}
