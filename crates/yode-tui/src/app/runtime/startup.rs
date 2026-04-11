use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{mpsc, Mutex};

use yode_core::context::AgentContext;
use yode_core::db::Database;
use yode_core::engine::{AgentEngine, EngineEvent};
use yode_core::permission::PermissionManager;
use yode_llm::provider::LlmProvider;
use yode_llm::registry::ProviderRegistry;
use yode_llm::types::Message;
use yode_tools::registry::ToolRegistry;

use super::super::scrollback::{print_entries_to_stdout, print_header_to_stdout};
use super::super::{App, ChatEntry, ChatRole, SkillCommandWrapper};

pub(super) struct RuntimeStartup {
    pub(super) app: App,
    pub(super) engine: Arc<Mutex<AgentEngine>>,
    pub(super) engine_event_tx: mpsc::UnboundedSender<EngineEvent>,
    pub(super) engine_event_rx: mpsc::UnboundedReceiver<EngineEvent>,
}

pub(super) async fn prepare_runtime(
    provider: Arc<dyn LlmProvider>,
    provider_registry: Arc<ProviderRegistry>,
    tools: Arc<ToolRegistry>,
    permissions: PermissionManager,
    context: AgentContext,
    db: Database,
    restored_messages: Option<Vec<Message>>,
    skill_commands: Vec<(String, String)>,
    all_provider_models: HashMap<String, Vec<String>>,
    startup_profile: Option<String>,
) -> Result<RuntimeStartup> {
    let resume_warmup_task = if context.is_resumed {
        let project_root = context.working_dir_compat();
        Some(tokio::task::spawn_blocking(move || {
            crate::commands::info::warm_resume_transcript_caches(&project_root)
        }))
    } else {
        None
    };

    let working_dir = context.working_dir_compat().display().to_string();
    let is_resumed = context.is_resumed;
    let provider_name = context.provider.clone();
    let provider_models = all_provider_models
        .get(&provider_name)
        .cloned()
        .unwrap_or_default();
    let mut app = App::new(
        context.model.clone(),
        context.session_id.clone(),
        working_dir,
        provider_name,
        provider_models,
        all_provider_models,
        provider_registry,
        Arc::clone(&tools),
    );
    app.session.startup_profile = startup_profile;
    app.cmd_completion.dynamic_commands = skill_commands.clone();

    crate::commands::register_all(&mut app.cmd_registry);
    register_skill_commands(&mut app, &skill_commands);

    print_header_to_stdout(&app)?;
    hydrate_restored_messages(&mut app, restored_messages.as_ref(), is_resumed);
    print_entries_to_stdout(&mut app)?;

    let mut engine_inner = AgentEngine::new(provider, Arc::clone(&tools), permissions, context);
    engine_inner.set_database(db);
    attach_hook_manager(&mut engine_inner);
    if let Some(messages) = &restored_messages {
        engine_inner.restore_messages(messages.clone());
    }
    engine_inner
        .initialize_session_hooks(if is_resumed { "resume" } else { "startup" })
        .await;

    let engine = Arc::new(Mutex::new(engine_inner));
    app.engine = Some(Arc::clone(&engine));
    let (engine_event_tx, engine_event_rx) = mpsc::unbounded_channel::<EngineEvent>();
    spawn_update_checker(engine_event_tx.clone());

    if let Some(task) = resume_warmup_task {
        app.session.resume_cache_warmup = Some(task.await?);
    }

    Ok(RuntimeStartup {
        app,
        engine,
        engine_event_tx,
        engine_event_rx,
    })
}

fn register_skill_commands(app: &mut App, skill_commands: &[(String, String)]) {
    for (name, description) in skill_commands {
        app.cmd_registry.register(Box::new(SkillCommandWrapper {
            meta: crate::commands::CommandMeta {
                name: Box::leak(name.clone().into_boxed_str()),
                description: Box::leak(description.clone().into_boxed_str()),
                aliases: &[],
                args: vec![],
                category: crate::commands::CommandCategory::Utility,
                hidden: false,
            },
        }));
    }
}

fn hydrate_restored_messages(
    app: &mut App,
    restored_messages: Option<&Vec<Message>>,
    is_resumed: bool,
) {
    if let Some(messages) = restored_messages {
        for message in messages {
            match message.role {
                yode_llm::types::Role::User => {
                    if let Some(content) = &message.content {
                        app.chat_entries
                            .push(ChatEntry::new(ChatRole::User, content.clone()));
                    }
                }
                yode_llm::types::Role::Assistant => {
                    if let Some(content) = &message.content {
                        app.chat_entries
                            .push(ChatEntry::new(ChatRole::Assistant, content.clone()));
                    }
                }
                _ => {}
            }
        }
        if is_resumed {
            app.chat_entries
                .push(ChatEntry::new(ChatRole::System, "Session resumed.".to_string()));
        }
    }
}

fn attach_hook_manager(engine: &mut AgentEngine) {
    if let Ok(config) = yode_core::config::Config::load() {
        if !config.hooks.hooks.is_empty() {
            use yode_core::hooks::{HookDefinition, HookManager};

            let mut hook_manager = HookManager::new(
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
            );
            for hook in &config.hooks.hooks {
                hook_manager.register(HookDefinition {
                    command: hook.command.clone(),
                    events: hook.events.clone(),
                    tool_filter: hook.tool_filter.clone(),
                    timeout_secs: hook.timeout_secs,
                    can_block: hook.can_block,
                });
            }
            engine.set_hook_manager(hook_manager);
        }
    }
}

fn spawn_update_checker(update_event_tx: mpsc::UnboundedSender<EngineEvent>) {
    tokio::spawn(async move {
        let config = match yode_core::config::Config::load() {
            Ok(config) => config,
            Err(_) => return,
        };

        if !config.update.auto_check {
            return;
        }

        let config_dir = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".yode");
        let updater = yode_core::updater::Updater::new(
            config_dir,
            config.update.auto_check,
            config.update.auto_download,
        );

        match updater.check_for_updates().await {
            Ok(Some(result)) => {
                let latest = result.latest_version.clone();
                let _ = update_event_tx.send(EngineEvent::UpdateAvailable(latest.clone()));
                if config.update.auto_download {
                    let _ = update_event_tx.send(EngineEvent::UpdateDownloading);
                    match updater.download_update(&result).await {
                        Ok(path) => {
                            tracing::info!("Update downloaded to: {:?}", path);
                            let _ = update_event_tx.send(EngineEvent::UpdateDownloaded(latest));
                        }
                        Err(error) => {
                            tracing::warn!("Update download failed: {}", error);
                        }
                    }
                }
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!("Update check failed: {}", error);
            }
        }
    });
}
