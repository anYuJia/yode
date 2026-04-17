use std::io;
use std::sync::Arc;
use std::time::Instant;

use crossterm::event::KeyModifiers;
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::error;

use yode_core::engine::{AgentEngine, EngineEvent};
use yode_tools::registry::ToolRegistry;

use super::engine_events::provider::reload_provider_from_config;
use super::scrollback::print_header_to_stdout;
use super::{App, ChatEntry, ChatRole, PermissionMode, TurnStatus};

pub(super) fn handle_enter(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    key: crossterm::event::KeyEvent,
    engine: &Arc<Mutex<AgentEngine>>,
    tools: &Arc<ToolRegistry>,
    engine_event_tx: &mpsc::UnboundedSender<EngineEvent>,
) {
    if app.cmd_completion.is_active() {
        if let Some(cmd) = app.cmd_completion.accept() {
            app.input.set_text(&cmd);
        }
        return;
    }
    if app.file_completion.is_active() {
        if let Some(path) = app.file_completion.accept() {
            let text = app.input.text();
            if let Some(at_pos) = text.rfind('@') {
                let new_text = format!("{}@{}", &text[..at_pos], path);
                app.input.set_text(&new_text);
            }
        }
        return;
    }

    let is_newline = key.modifiers.contains(KeyModifiers::SHIFT);
    if is_newline {
        app.input.insert_newline();
        return;
    }

    let is_empty = app.input.text().trim().is_empty();
    if is_empty {
        if let Some(ghost) = app.input.ghost_text.take() {
            app.input.set_text(&ghost);
        } else {
            return;
        }
    }

    let (display, payload, raw_typed) = app.input.take();
    if payload.trim().is_empty() {
        return;
    }

    app.history.push(raw_typed.clone());
    app.cmd_completion.close();
    app.file_completion.close();

    if app.wizard.is_some() {
        return;
    }

    if app.handle_shell_command(&raw_typed) {
        return;
    }

    if raw_typed.starts_with('/') {
        let trimmed = raw_typed.trim();
        let (cmd_name, cmd_args) = match trimmed.find(' ') {
            Some(pos) => (&trimmed[1..pos], trimmed[pos + 1..].trim()),
            None => (&trimmed[1..], ""),
        };

        if app.cmd_registry.find(cmd_name).is_none() {
            if let Some(suggestion) = app.cmd_registry.suggest_similar(cmd_name) {
                app.chat_entries.push(ChatEntry::new(
                    ChatRole::System,
                    format!(
                        "Unknown command: /{}. Did you mean /{}?",
                        cmd_name, suggestion
                    ),
                ));
            } else {
                app.chat_entries.push(ChatEntry::new(
                    ChatRole::System,
                    format!(
                        "Unknown command: /{}. Type /help for available commands.",
                        cmd_name
                    ),
                ));
            }
            return;
        }

        if cmd_name != "clear" {
            app.chat_entries
                .push(ChatEntry::new(ChatRole::User, raw_typed.clone()));
        }

        let result = {
            let mut ctx = crate::commands::context::CommandContext {
                engine: engine.clone(),
                provider_registry: &app.provider_registry,
                provider_name: &mut app.provider_name,
                provider_models: &mut app.provider_models,
                all_provider_models: &app.all_provider_models,
                chat_entries: &mut app.chat_entries,
                printed_count: &mut app.printed_count,
                streaming_buf: &mut app.streaming_buf,
                streaming_printed_lines: &mut app.streaming_printed_lines,
                streaming_in_code_block: &mut app.streaming_in_code_block,
                streaming_code_block_language: &mut app.streaming_code_block_language,
                streaming_shell_session_state: &mut app.streaming_shell_session_state,
                tools,
                session: &mut app.session,
                input: &mut app.input,
                terminal_caps: &app.terminal_caps,
                input_history: &app.history.entries(),
                should_quit: &mut app.should_quit,
                session_start: app.session_start,
                turn_started_at: app.turn_started_at,
                cmd_registry: &app.cmd_registry,
                engine_event_tx,
            };
            app.cmd_registry
                .execute_command(cmd_name, cmd_args, &mut ctx)
        };

        if cmd_name == "clear" {
            let mut stdout = io::stdout();
            let _ = stdout.execute(crossterm::terminal::Clear(
                crossterm::terminal::ClearType::All,
            ));
            let _ = stdout.execute(crossterm::cursor::MoveTo(0, 0));

            let _ = print_header_to_stdout(app);

            if let Ok((_cols, rows)) = crossterm::cursor::position() {
                let area = terminal.get_frame().area();
                let new_area = ratatui::layout::Rect {
                    x: area.x,
                    y: rows,
                    width: area.width,
                    height: area.height,
                };
                terminal.set_viewport_area(new_area);
            }

            let _ = terminal.clear();
            let _ = terminal.draw(|f| {
                crate::ui::render(f, app);
            });
            return;
        }

        use crate::app::InspectorView;
        use crate::commands::CommandOutput;
        match result {
            Some(Ok(CommandOutput::Message(msg))) => {
                app.chat_entries.push(ChatEntry::new(ChatRole::System, msg));
            }
            Some(Ok(CommandOutput::Messages(msgs))) => {
                for msg in msgs {
                    app.chat_entries.push(ChatEntry::new(ChatRole::System, msg));
                }
            }
            Some(Ok(CommandOutput::Silent)) => {}
            Some(Ok(CommandOutput::OpenInspector(document))) => {
                app.inspector.stack.push(document.state.title.clone());
                app.inspector.views.push(InspectorView { document });
            }
            Some(Ok(CommandOutput::StartWizard(wizard))) => {
                app.wizard = Some(wizard);
            }
            Some(Ok(CommandOutput::ReloadProvider { name, messages })) => {
                for msg in messages {
                    app.chat_entries.push(ChatEntry::new(ChatRole::System, msg));
                }
                reload_provider_from_config(&name, app);
            }
            Some(Err(e)) => {
                app.chat_entries.push(ChatEntry::new(ChatRole::Error, e));
            }
            None => {
                app.chat_entries.push(ChatEntry::new(
                    ChatRole::System,
                    format!(
                        "Unknown command: /{}. Type /help for available commands.",
                        cmd_name
                    ),
                ));
            }
        }
        return;
    }

    let processed_payload = app.process_file_references(&payload);
    let processed_display = app.process_file_references(&display);

    if app.session.permission_mode == PermissionMode::Plan {
        app.chat_entries
            .push(ChatEntry::new(ChatRole::User, processed_display.clone()));
        app.chat_entries.push(ChatEntry::new(
            ChatRole::System,
            "[Plan mode] Input recorded. Switch to Normal or Auto-Accept to execute.".to_string(),
        ));
    } else {
        send_input(
            app,
            &processed_display,
            &processed_payload,
            engine,
            engine_event_tx,
        );
    }
}

pub(super) fn send_input(
    app: &mut App,
    display: &str,
    payload: &str,
    engine: &Arc<Mutex<AgentEngine>>,
    engine_event_tx: &mpsc::UnboundedSender<EngineEvent>,
) {
    app.pending_inputs
        .push((display.to_string(), payload.to_string()));
    try_process_next(app, engine, engine_event_tx);
}

pub(super) fn try_process_next(
    app: &mut App,
    engine: &Arc<Mutex<AgentEngine>>,
    engine_event_tx: &mpsc::UnboundedSender<EngineEvent>,
) {
    if app.is_processing || app.pending_inputs.is_empty() {
        return;
    }

    let (display, payload) = app.pending_inputs.remove(0);
    app.is_processing = true;
    app.chat_entries
        .push(ChatEntry::new(ChatRole::User, display));

    let cancel_token = CancellationToken::new();
    app.thinking.start(cancel_token.clone());
    app.turn_started_at = Some(Instant::now());
    app.turn_tool_count = 0;
    app.session.turn_input_tokens = 0;
    app.session.turn_output_tokens = 0;

    let new_bytes = payload.len();
    app.session.turn_input_tokens = (new_bytes as u32 / 3).max(1);
    app.session.turn_output_tokens = 0;

    let verb = {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        Instant::now().hash(&mut hasher);
        let idx = hasher.finish() as usize % crate::app::SPINNER_VERBS.len();
        crate::app::SPINNER_VERBS[idx]
    };
    app.turn_status = TurnStatus::Working { verb };
    app.sync_thinking();

    let (confirm_tx, confirm_rx) = mpsc::unbounded_channel();
    app.confirm_tx = Some(confirm_tx);

    let engine_clone = Arc::clone(engine);
    let event_tx_clone = engine_event_tx.clone();
    tokio::spawn(async move {
        let mut eng = engine_clone.lock().await;
        let result = eng
            .run_turn_streaming(
                &payload,
                yode_core::context::QuerySource::User,
                event_tx_clone.clone(),
                confirm_rx,
                Some(cancel_token),
            )
            .await;
        if let Err(e) = result {
            error!("Engine turn error: {}", e);
            let _ = event_tx_clone.send(EngineEvent::Error(format!("Engine error: {}", e)));
            let _ = event_tx_clone.send(EngineEvent::Done);
        }
    });
}
