use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::{mpsc, Mutex};

use yode_core::context::AgentContext;
use yode_core::db::Database;
use yode_core::engine::{AgentEngine, EngineEvent};
use yode_core::permission::PermissionManager;
use yode_llm::provider::LlmProvider;
use yode_llm::registry::ProviderRegistry;
use yode_llm::types::Message;
use yode_tools::registry::ToolRegistry;

use crate::event::{self, AppEvent};
use crate::ui;

use super::engine_events::handle_engine_event;
use super::lifecycle::print_exit_summary;
use super::scrollback::{
    flush_entries_to_scrollback, print_entries_to_stdout, print_header_to_stdout,
};
use super::{
    handle_key_event, push_grouped_system_entry, App, ChatEntry, ChatRole, SkillCommandWrapper,
};

/// Run the TUI application.
pub async fn run(
    provider: Arc<dyn LlmProvider>,
    provider_registry: Arc<ProviderRegistry>,
    tools: Arc<ToolRegistry>,
    permissions: PermissionManager,
    context: AgentContext,
    db: Database,
    restored_messages: Option<Vec<Message>>,
    skill_commands: Vec<(String, String)>,
    all_provider_models: HashMap<String, Vec<String>>,
) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnableBracketedPaste)?;
    stdout.execute(crossterm::style::Print("\n"))?;

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
        tools.clone(),
    );
    if is_resumed {
        app.session.resume_cache_warmup = Some(crate::commands::info::warm_resume_transcript_caches(
            &context.working_dir_compat(),
        ));
    }
    app.cmd_completion.dynamic_commands = skill_commands.clone();

    crate::commands::register_all(&mut app.cmd_registry);

    for (name, description) in &skill_commands {
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

    print_header_to_stdout(&app)?;

    if let Some(ref messages) = restored_messages {
        for msg in messages {
            match msg.role {
                yode_llm::types::Role::User => {
                    if let Some(ref content) = msg.content {
                        app.chat_entries
                            .push(ChatEntry::new(ChatRole::User, content.clone()));
                    }
                }
                yode_llm::types::Role::Assistant => {
                    if let Some(ref content) = msg.content {
                        app.chat_entries
                            .push(ChatEntry::new(ChatRole::Assistant, content.clone()));
                    }
                }
                _ => {}
            }
        }
    }

    print_entries_to_stdout(&mut app)?;

    let mut engine_inner = AgentEngine::new(provider, tools.clone(), permissions, context);
    engine_inner.set_database(db);

    if let Ok(config) = yode_core::config::Config::load() {
        if !config.hooks.hooks.is_empty() {
            use yode_core::hooks::{HookDefinition, HookManager};

            let mut hook_mgr = HookManager::new(
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
            );
            for h in &config.hooks.hooks {
                hook_mgr.register(HookDefinition {
                    command: h.command.clone(),
                    events: h.events.clone(),
                    tool_filter: h.tool_filter.clone(),
                    timeout_secs: h.timeout_secs,
                    can_block: h.can_block,
                });
            }
            engine_inner.set_hook_manager(hook_mgr);
        }
    }

    if let Some(ref messages) = restored_messages {
        engine_inner.restore_messages(messages.clone());
        if is_resumed {
            app.chat_entries.push(ChatEntry::new(
                ChatRole::System,
                "Session resumed.".to_string(),
            ));
        }
    }
    engine_inner
        .initialize_session_hooks(if is_resumed { "resume" } else { "startup" })
        .await;

    let engine = Arc::new(Mutex::new(engine_inner));
    app.engine = Some(engine.clone());
    let (engine_event_tx, mut engine_event_rx) = mpsc::unbounded_channel::<EngineEvent>();

    let update_event_tx = engine_event_tx.clone();
    tokio::spawn(async move {
        let config = match yode_core::config::Config::load() {
            Ok(c) => c,
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
                            let _ =
                                update_event_tx.send(EngineEvent::UpdateDownloaded(latest.clone()));
                        }
                        Err(e) => {
                            tracing::warn!("Update download failed: {}", e);
                        }
                    }
                }
                tracing::info!("New version available: {}", latest);
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("Update check failed: {}", e);
            }
        }
    });

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::with_options(
        backend,
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Inline(4),
        },
    )?;

    let result = run_app(
        &mut terminal,
        &mut app,
        engine,
        tools,
        engine_event_tx,
        &mut engine_event_rx,
    )
    .await;

    terminal.clear()?;

    disable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(DisableBracketedPaste)?;

    let area = terminal.get_frame().area();
    crossterm::execute!(stdout, crossterm::cursor::MoveTo(0, area.bottom()))?;
    println!();

    print_exit_summary(&app);

    if let Err(ref e) = result {
        eprintln!("Yode error: {:#}", e);
    }
    result
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    engine: Arc<Mutex<AgentEngine>>,
    tools: Arc<ToolRegistry>,
    engine_event_tx: mpsc::UnboundedSender<EngineEvent>,
    engine_event_rx: &mut mpsc::UnboundedReceiver<EngineEvent>,
) -> Result<()> {
    loop {
        app.sync_thinking();

        while let Ok(event) = engine_event_rx.try_recv() {
            handle_engine_event(app, event, &engine, &engine_event_tx);
        }
        if let Ok(engine_guard) = engine.try_lock() {
            for notification in engine_guard.drain_runtime_task_notifications() {
                app.chat_entries.push(ChatEntry::new(
                    ChatRole::System,
                    format!(
                        "[Task:{}] {}",
                        notification.severity.label(),
                        notification.message
                    ),
                ));
            }
            if app.last_task_brief_time.elapsed() >= Duration::from_secs(45) {
                let running = engine_guard
                    .runtime_tasks_snapshot()
                    .into_iter()
                    .filter(|task| matches!(task.status, yode_tools::RuntimeTaskStatus::Running))
                    .collect::<Vec<_>>();
                if !running.is_empty() {
                    let mut lines = vec!["Background tasks still running:".to_string()];
                    for task in running.into_iter().take(3) {
                        lines.push(format!(
                            "  - {} [{}] {}{}",
                            task.id,
                            task.kind,
                            task.description,
                            task
                                .last_progress
                                .as_ref()
                                .map(|progress| format!(" — {}", progress))
                                .unwrap_or_default()
                        ));
                    }
                    push_grouped_system_entry(
                        app,
                        "Background tasks still running",
                        lines.join("\n"),
                    );
                    app.last_task_brief_time = Instant::now();
                }
            }
        }

        crossterm::execute!(
            terminal.backend_mut(),
            crossterm::terminal::BeginSynchronizedUpdate
        )?;

        flush_entries_to_scrollback(terminal, app)?;

        {
            let needed = if app.wizard.is_some() {
                app.wizard.as_ref().unwrap().viewport_height() + 1
            } else if app.pending_confirmation.is_some() {
                4u16
            } else {
                let term_width = terminal.get_frame().area().width;
                let visual_lines = app.input.visual_line_count(term_width) as u16;
                let completion_lines = if app.cmd_completion.is_active() {
                    if app.cmd_completion.args_hint.is_some() {
                        1
                    } else if !app.cmd_completion.candidates.is_empty() {
                        5
                    } else {
                        0
                    }
                } else {
                    0
                };
                let thinking_line: u16 = if completion_lines > 0 {
                    0
                } else if app.turn_status.is_visible() {
                    3
                } else {
                    0
                };
                let pending_line = app.pending_inputs.len() as u16;
                visual_lines.clamp(1, 5) + completion_lines + thinking_line + pending_line + 4
            };
            let area = terminal.get_frame().area();
            if area.height != needed {
                if needed > area.height {
                    let grow_by = needed - area.height;
                    crossterm::execute!(
                        terminal.backend_mut(),
                        crossterm::terminal::ScrollUp(grow_by)
                    )?;
                    let new_y = area.y.saturating_sub(grow_by);
                    let new_area = ratatui::layout::Rect {
                        x: area.x,
                        y: new_y,
                        width: area.width,
                        height: needed,
                    };
                    terminal.viewport = ratatui::Viewport::Inline(needed);
                    terminal.set_viewport_area(new_area);
                } else {
                    let shrink_by = area.height - needed;
                    let new_y = area.bottom().saturating_sub(needed);

                    for row in area.y..new_y {
                        crossterm::execute!(
                            terminal.backend_mut(),
                            crossterm::cursor::MoveTo(0, row),
                            crossterm::terminal::Clear(
                                crossterm::terminal::ClearType::CurrentLine
                            )
                        )?;
                    }

                    crossterm::execute!(
                        terminal.backend_mut(),
                        crossterm::terminal::ScrollDown(shrink_by)
                    )?;

                    let new_area = ratatui::layout::Rect {
                        x: area.x,
                        y: new_y,
                        width: area.width,
                        height: needed,
                    };
                    terminal.viewport = ratatui::Viewport::Inline(needed);
                    terminal.set_viewport_area(new_area);
                }
                terminal.clear()?;
            }
        }

        terminal.draw(|f| {
            ui::render(f, app);
        })?;

        crossterm::execute!(
            terminal.backend_mut(),
            crossterm::terminal::EndSynchronizedUpdate
        )?;

        if app.should_quit {
            break;
        }

        if let Some(app_event) = event::poll_event(Duration::from_millis(50))? {
            match app_event {
                AppEvent::Key(key) => {
                    handle_key_event(terminal, app, key, &engine, &tools, &engine_event_tx);
                }
                AppEvent::Paste(text) => {
                    let text = text.replace("\r\n", "\n").replace('\r', "\n");
                    if let Some(ref mut wiz) = app.wizard {
                        for c in text.chars() {
                            if c != '\n' && c != '\r' {
                                wiz.input_char(c);
                            }
                        }
                    } else if super::input::should_fold_paste(&text) {
                        app.input.insert_attachment(text);
                    } else {
                        for line in text.split_inclusive('\n') {
                            let clean = line.trim_end_matches('\n');
                            for c in clean.chars() {
                                app.input.insert_char(c);
                            }
                            if line.ends_with('\n') {
                                app.input.insert_newline();
                            }
                        }
                    }
                }
                AppEvent::Resize(_, _) => {}
                AppEvent::Tick => {
                    if app.is_thinking {
                        app.thinking.advance_spinner();
                    }
                }
            }
        }
    }

    {
        let mut engine = engine.lock().await;
        engine.finalize_session_hooks("tui_exit").await;
    }

    Ok(())
}
