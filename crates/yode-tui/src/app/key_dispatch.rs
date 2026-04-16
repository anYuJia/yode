use std::io;
use std::sync::Arc;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::{mpsc, Mutex};

use yode_core::engine::{AgentEngine, ConfirmResponse, EngineEvent};
use yode_tools::registry::ToolRegistry;

use crate::event;
use crate::commands::artifact_nav::record_inspector_action_history;

use super::engine_events::provider::{reload_provider_from_config, switch_provider_from_config};
use super::key_handlers::{handle_char, handle_down, handle_tab, handle_up};
use super::turn_flow::handle_enter;
use super::{input, App, ChatEntry, ChatRole};

/// Centralized key event handler.
pub(super) fn handle_key_event(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    key: crossterm::event::KeyEvent,
    engine: &Arc<Mutex<AgentEngine>>,
    tools: &Arc<ToolRegistry>,
    engine_event_tx: &mpsc::UnboundedSender<EngineEvent>,
) {
    if app.wizard.is_some() {
        use super::wizard::WizardStep;

        match key.code {
            KeyCode::Esc => {
                app.wizard = None;
                app.chat_entries
                    .push(ChatEntry::new(ChatRole::System, "Wizard cancelled.".into()));
            }
            KeyCode::Up => {
                if let Some(ref mut wiz) = app.wizard {
                    wiz.select_up();
                }
            }
            KeyCode::Down => {
                if let Some(ref mut wiz) = app.wizard {
                    wiz.select_down();
                }
            }
            KeyCode::Char(c) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && c == 'c' {
                    app.wizard = None;
                    app.chat_entries
                        .push(ChatEntry::new(ChatRole::System, "Wizard cancelled.".into()));
                } else if let Some(ref mut wiz) = app.wizard {
                    if matches!(wiz.current_step(), Some(WizardStep::Input { .. })) {
                        wiz.input_char(c);
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(ref mut wiz) = app.wizard {
                    if matches!(wiz.current_step(), Some(WizardStep::Input { .. })) {
                        wiz.input_backspace();
                    }
                }
            }
            KeyCode::Enter => {
                let result = app.wizard.as_mut().unwrap().submit();
                match result {
                    Ok(None) => {}
                    Ok(Some(messages)) => {
                        let next_wizard = app
                            .wizard
                            .as_mut()
                            .and_then(|w| w.next_wizard.take())
                            .map(|wizard| *wizard);
                        let apply_provider =
                            app.wizard.as_ref().and_then(|w| w.apply_provider.clone());
                        let reload_name =
                            app.wizard.as_ref().and_then(|w| w.reload_provider.clone());
                        let apply_model =
                            app.wizard.as_ref().and_then(|w| w.apply_model.clone());
                        for msg in messages {
                            app.chat_entries.push(ChatEntry::new(ChatRole::System, msg));
                        }
                        if let Some(name) = apply_provider {
                            if let Err(err) = switch_provider_from_config(&name, app) {
                                app.chat_entries.push(ChatEntry::new(ChatRole::Error, err));
                            }
                        }
                        if let Some(name) = reload_name {
                            reload_provider_from_config(&name, app);
                        }
                        if let Some(model) = apply_model {
                            if let Ok(mut eng) = engine.try_lock() {
                                eng.set_model(model.clone());
                            }
                            app.session.model = model.clone();
                        }
                        app.wizard = next_wizard;
                    }
                    Err(e) => {
                        app.chat_entries.push(ChatEntry::new(ChatRole::Error, e));
                        app.wizard = None;
                    }
                }
            }
            _ => {}
        }
        return;
    }

    if let Some(inspector) = app.inspector.views.last_mut() {
        match key.code {
            KeyCode::Esc => {
                app.inspector.views.pop();
                app.inspector.stack.pop();
            }
            KeyCode::Up => inspector.document.move_up(),
            KeyCode::Down => inspector.document.move_down(),
            KeyCode::PageUp => inspector.document.page_up(10),
            KeyCode::PageDown => inspector.document.page_down(10),
            KeyCode::Left if matches!(inspector.document.state.focus, crate::ui::inspector::InspectorFocus::Actions) => {
                inspector.document.cycle_action_prev();
            }
            KeyCode::Right if matches!(inspector.document.state.focus, crate::ui::inspector::InspectorFocus::Actions) => {
                inspector.document.cycle_action_next();
            }
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    inspector.document.toggle_focus();
                } else {
                    inspector.document.cycle_tab();
                }
            }
            KeyCode::Char('/') => inspector.document.begin_search(),
            KeyCode::Backspace if inspector.document.state.search_active => {
                inspector.document.pop_search_char();
            }
            KeyCode::Enter if inspector.document.state.search_active => {
                inspector.document.finish_search(true);
            }
            KeyCode::Enter => {
                if let Some(command) = inspector.document.handoff_command() {
                    let execute_now = key.modifiers.contains(KeyModifiers::CONTROL);
                    if let Some(panel) = inspector.document.active_panel() {
                        if matches!(inspector.document.state.focus, crate::ui::inspector::InspectorFocus::Actions)
                            && !panel.actions.is_empty()
                        {
                            let index = inspector
                                .document
                                .state
                                .selected_action
                                .min(panel.actions.len().saturating_sub(1));
                            inspector
                                .document
                                .note_action_dispatched(panel.actions[index].label.clone());
                        }
                    }
                    if execute_now {
                        let _ = record_inspector_action_history(
                            std::path::Path::new(&app.session.working_dir),
                            &app.session.session_id,
                            &command,
                        );
                    }
                    app.input.set_text(&command);
                    app.inspector.views.pop();
                    app.inspector.stack.pop();
                    if execute_now {
                        super::turn_flow::handle_enter(
                            terminal,
                            app,
                            crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                            engine,
                            tools,
                            engine_event_tx,
                        );
                    }
                }
            }
            KeyCode::Char(c) if inspector.document.state.search_active => {
                inspector.document.append_search_char(c);
            }
            KeyCode::Home => inspector.document.jump_to_line(1),
            KeyCode::End => inspector.document.page_down(10_000),
            _ => {}
        }
        return;
    }

    if app.history.is_searching() {
        match key.code {
            KeyCode::Esc => {
                app.history.exit_search(false);
            }
            KeyCode::Enter => {
                if let Some(text) = app.history.exit_search(true) {
                    app.input.set_text(&text);
                }
            }
            KeyCode::Char(c) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && c == 'r' {
                    app.history.search_next();
                } else {
                    app.history.append_search_char(c);
                }
            }
            KeyCode::Backspace => {
                app.history.pop_search_char();
            }
            _ => {}
        }
        return;
    }

    if key.code == KeyCode::Esc {
        if app.is_thinking {
            app.cancel_generation();
        } else if app.cmd_completion.is_active() {
            app.cmd_completion.close();
        } else if app.file_completion.is_active() {
            app.file_completion.close();
        }
        return;
    }

    if event::is_quit(&key) {
        if app.is_thinking {
            app.cancel_generation();
            app.last_ctrl_c = Some(Instant::now());
        } else {
            let now = Instant::now();
            let is_double_tap = app
                .last_ctrl_c
                .map(|t| now.duration_since(t).as_millis() < 500)
                .unwrap_or(false);

            if is_double_tap {
                app.should_quit = true;
            } else if app.input.text().trim().is_empty() {
                app.chat_entries.push(ChatEntry::new(
                    ChatRole::System,
                    "Press Ctrl+C again to quit".to_string(),
                ));
                app.last_ctrl_c = Some(now);
            } else {
                app.input.clear();
                app.last_ctrl_c = Some(now);
            }
        }
        return;
    }

    if app.pending_confirmation.is_some() {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Char('1') => {
                if let Some(tx) = &app.confirm_tx {
                    let _ = tx.send(ConfirmResponse::Allow);
                }
                app.pending_confirmation = None;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('3') => {
                if let Some(tx) = &app.confirm_tx {
                    let _ = tx.send(ConfirmResponse::Deny);
                }
                app.pending_confirmation = None;
            }
            KeyCode::Char('a') | KeyCode::Char('A') | KeyCode::Char('2') => {
                if let Some(ref confirm) = app.pending_confirmation {
                    if !app.session.always_allow_tools.contains(&confirm.name) {
                        app.session.always_allow_tools.push(confirm.name.clone());
                    }
                }
                if let Some(tx) = &app.confirm_tx {
                    let _ = tx.send(ConfirmResponse::Allow);
                }
                app.pending_confirmation = None;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if app.confirm_selected > 0 {
                    app.confirm_selected -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if app.confirm_selected < 2 {
                    app.confirm_selected += 1;
                }
            }
            KeyCode::Enter => {
                match app.confirm_selected {
                    0 => {
                        if let Some(tx) = &app.confirm_tx {
                            let _ = tx.send(ConfirmResponse::Allow);
                        }
                    }
                    1 => {
                        if let Some(ref confirm) = app.pending_confirmation {
                            if !app.session.always_allow_tools.contains(&confirm.name) {
                                app.session.always_allow_tools.push(confirm.name.clone());
                            }
                        }
                        if let Some(tx) = &app.confirm_tx {
                            let _ = tx.send(ConfirmResponse::Allow);
                        }
                    }
                    _ => {
                        if let Some(tx) = &app.confirm_tx {
                            let _ = tx.send(ConfirmResponse::Deny);
                        }
                    }
                }
                app.pending_confirmation = None;
            }
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Enter => handle_enter(terminal, app, key, engine, tools, engine_event_tx),
        KeyCode::Char(c)
            if (key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::SUPER))
                && c == 'v' =>
        {
            if let Ok(output) = std::process::Command::new("pbpaste").output() {
                if output.status.success() {
                    let text = String::from_utf8_lossy(&output.stdout).to_string();
                    let text = text.replace("\r\n", "\n").replace('\r', "\n");
                    if !text.is_empty() {
                        if input::should_fold_paste(&text) {
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
                }
            }
        }
        KeyCode::Char(c) => handle_char(app, key, c),
        KeyCode::Backspace => {
            app.input.backspace();
            let ctx = crate::commands::context::CompletionContext {
                provider_models: &app.provider_models,
                all_provider_models: &app.all_provider_models,
                provider_name: &app.provider_name,
                tools: &app.tools,
                working_dir: &app.session.working_dir,
            };
            app.cmd_completion.update(
                &app.input.lines[0],
                !app.input.is_multiline(),
                &app.cmd_registry,
                &ctx,
            );
            app.file_completion.update(&app.input.text());
        }
        KeyCode::Delete => app.input.delete(),
        KeyCode::Left => app.input.move_left(),
        KeyCode::Right => app.input.move_right(),
        KeyCode::Up => handle_up(app),
        KeyCode::Down => handle_down(app),
        KeyCode::Home => app.input.move_home(),
        KeyCode::End => {
            app.input.move_end();
        }
        KeyCode::BackTab => {
            if app.file_completion.is_active() {
                app.file_completion.cycle_back();
            } else if app.cmd_completion.is_active() {
                app.cmd_completion.cycle_back();
            } else {
                app.session.permission_mode = app.session.permission_mode.next();
            }
        }
        KeyCode::Tab => handle_tab(app),
        KeyCode::PageUp => {}
        KeyCode::PageDown => {}
        _ => {}
    }
}
