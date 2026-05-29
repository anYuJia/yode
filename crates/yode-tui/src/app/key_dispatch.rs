use std::io;
use std::sync::Arc;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::{mpsc, Mutex};

use yode_core::engine::{AgentEngine, ConfirmResponse, EngineEvent};
use yode_tools::registry::ToolRegistry;

use crate::commands::artifact_nav::record_inspector_action_history;
use crate::commands::inspector_bridge::document_from_command_output;
use crate::commands::CommandOutput;
use crate::event;
use crate::ui::inspector::InspectorActionEffect;

use super::detail_inspector::{
    INSPECTOR_CONFIRM_ALLOW, INSPECTOR_CONFIRM_ALWAYS, INSPECTOR_CONFIRM_DENY,
};
use super::engine_events::provider::{reload_provider_from_config, switch_provider_from_config};
use super::key_handlers::{handle_char, handle_down, handle_tab, handle_up};
use super::turn_flow::handle_enter;
use super::{
    input, open_latest_tool_inspector, open_pending_confirmation_inspector, push_system_entry, App,
    ChatEntry, ChatRole, InspectorView,
};

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
                push_system_entry(app, "Wizard cancelled.");
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
                    push_system_entry(app, "Wizard cancelled.");
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
                let Some(wizard) = app.wizard.as_mut() else {
                    return;
                };
                let result = wizard.submit();
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
                        let apply_model = app.wizard.as_ref().and_then(|w| w.apply_model.clone());
                        for msg in messages {
                            push_system_entry(app, msg);
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
            KeyCode::Left
                if matches!(
                    inspector.document.state.focus,
                    crate::ui::inspector::InspectorFocus::Actions
                ) =>
            {
                inspector.document.cycle_action_prev();
            }
            KeyCode::Right
                if matches!(
                    inspector.document.state.focus,
                    crate::ui::inspector::InspectorFocus::Actions
                ) =>
            {
                inspector.document.cycle_action_next();
            }
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    inspector.document.toggle_focus();
                } else {
                    inspector.document.cycle_tab();
                }
            }
            KeyCode::BackTab => inspector.document.toggle_focus(),
            KeyCode::Char('/') => inspector.document.begin_search(),
            KeyCode::Backspace if inspector.document.state.search_active => {
                inspector.document.pop_search_char();
            }
            KeyCode::Enter if inspector.document.state.search_active => {
                inspector.document.finish_search(true);
            }
            KeyCode::Enter => {
                let execute_now = key.modifiers.contains(KeyModifiers::CONTROL);
                let action = inspector.document.handoff_action();
                let command = action
                    .as_ref()
                    .map(|action| action.command.clone())
                    .or_else(|| inspector.document.handoff_command());
                let label = action.as_ref().map(|action| action.label.clone());
                let effect = action
                    .as_ref()
                    .map(|action| action.effect(execute_now))
                    .or_else(|| {
                        command.as_ref().map(|command| {
                            if execute_now {
                                InspectorActionEffect::RunCommand(command.clone())
                            } else {
                                InspectorActionEffect::LoadCommand(command.clone())
                            }
                        })
                    });

                let Some(effect) = effect else {
                    return;
                };

                if let Some(label) = label.as_deref() {
                    inspector.document.note_action_dispatched(label);
                }
                let fallback_detail = inspector_action_fallback_detail(&effect);

                let _ = inspector;
                match effect {
                    InspectorActionEffect::InternalConfirmAllow
                    | InspectorActionEffect::InternalConfirmAlways
                    | InspectorActionEffect::InternalConfirmDeny => {
                        let label = label.unwrap_or_else(|| "confirmation action".to_string());
                        let result = execute_inspector_typed_action(app, &effect);
                        if let Some(active) = app.inspector.views.last_mut() {
                            match result {
                                Ok(()) => active.document.note_action_succeeded(&label),
                                Err(reason) => {
                                    active.document.note_action_failed(&label, reason);
                                    return;
                                }
                            }
                        }
                        app.inspector.views.pop();
                        app.inspector.stack.pop();
                    }
                    InspectorActionEffect::LoadCommand(command) => {
                        let should_execute = false;
                        if execute_inspector_internal_action(app, &command) {
                            if let Some(label) = label {
                                if let Some(active) = app.inspector.views.last_mut() {
                                    active.document.note_action_succeeded(label);
                                }
                            }
                            app.inspector.views.pop();
                            app.inspector.stack.pop();
                            return;
                        }
                        if should_execute {
                            let _ = record_inspector_action_history(
                                std::path::Path::new(&app.session.working_dir),
                                &app.session.session_id,
                                &command,
                            );
                        }
                        app.input.set_text(&command);
                        if let Some(label) = label {
                            if let Some(active) = app.inspector.views.last_mut() {
                                if let Some(detail) = fallback_detail {
                                    active
                                        .document
                                        .note_action_succeeded_with_detail(label, detail);
                                } else {
                                    active.document.note_action_succeeded(label);
                                }
                            }
                        }
                        app.inspector.views.pop();
                        app.inspector.stack.pop();
                    }
                    InspectorActionEffect::RunCommand(command) => {
                        let should_execute = true;
                        if execute_inspector_internal_action(app, &command) {
                            if let Some(label) = label {
                                if let Some(active) = app.inspector.views.last_mut() {
                                    active.document.note_action_succeeded(label);
                                }
                            }
                            app.inspector.views.pop();
                            app.inspector.stack.pop();
                            return;
                        }
                        if should_execute {
                            let _ = record_inspector_action_history(
                                std::path::Path::new(&app.session.working_dir),
                                &app.session.session_id,
                                &command,
                            );
                        }
                        app.input.set_text(&command);
                        if let Some(label) = label {
                            if let Some(active) = app.inspector.views.last_mut() {
                                if let Some(detail) = fallback_detail {
                                    active
                                        .document
                                        .note_action_succeeded_with_detail(label, detail);
                                } else {
                                    active.document.note_action_succeeded(label);
                                }
                            }
                        }
                        app.inspector.views.pop();
                        app.inspector.stack.pop();
                        if should_execute {
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
                    InspectorActionEffect::OpenArtifact { target, command }
                    | InspectorActionEffect::OpenInspectorTarget { target, command } => {
                        let label = label.unwrap_or_else(|| "open inspector target".to_string());
                        match execute_inspector_open_action(
                            app,
                            &effect_from_target(&target, &command),
                            engine,
                            tools,
                            engine_event_tx,
                        ) {
                            Ok(document) => {
                                if let Some(active) = app.inspector.views.last_mut() {
                                    active.document.note_action_succeeded(&label);
                                }
                                app.inspector.views.pop();
                                app.inspector.stack.pop();
                                app.inspector.stack.push(document.state.title.clone());
                                app.inspector.views.push(InspectorView { document });
                            }
                            Err(reason) if reason.contains("has no target") => {
                                if let Some(active) = app.inspector.views.last_mut() {
                                    active.document.note_action_failed(label, reason);
                                }
                            }
                            Err(_) => {
                                let _ = record_inspector_action_history(
                                    std::path::Path::new(&app.session.working_dir),
                                    &app.session.session_id,
                                    &command,
                                );
                                app.input.set_text(&command);
                                if let Some(active) = app.inspector.views.last_mut() {
                                    let detail = fallback_detail.unwrap_or("fallback ran command");
                                    active
                                        .document
                                        .note_action_succeeded_with_detail(label, detail);
                                }
                                app.inspector.views.pop();
                                app.inspector.stack.pop();
                                super::turn_flow::handle_enter(
                                    terminal,
                                    app,
                                    crossterm::event::KeyEvent::new(
                                        KeyCode::Enter,
                                        KeyModifiers::NONE,
                                    ),
                                    engine,
                                    tools,
                                    engine_event_tx,
                                );
                            }
                        }
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
        if app.pending_confirmation.is_some() {
            if let Some(tx) = &app.confirm_tx {
                let _ = tx.send(ConfirmResponse::Deny);
            }
            app.pending_confirmation = None;
            return;
        }
        if app.chat_scroll_active {
            app.chat_scroll_active = false;
            app.chat_scroll_offset = 0;
            return;
        }
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
            KeyCode::Tab => {
                amend_pending_confirmation(app);
            }
            KeyCode::Char('e')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    || key.modifiers.contains(KeyModifiers::SUPER) =>
            {
                explain_pending_confirmation(app);
            }
            KeyCode::Char('o')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    || key.modifiers.contains(KeyModifiers::SUPER) =>
            {
                let _ = open_pending_confirmation_inspector(app);
            }
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
            KeyCode::Up | KeyCode::Char('k') if app.confirm_selected > 0 => {
                app.confirm_selected -= 1;
            }
            KeyCode::Down | KeyCode::Char('j') if app.confirm_selected < 2 => {
                app.confirm_selected += 1;
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
        KeyCode::Char('o')
            if (key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::SUPER)) =>
        {
            let _ = open_latest_tool_inspector(app);
        }
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
        KeyCode::End if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.chat_scroll_offset = 0;
            app.chat_scroll_active = false;
        }
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
        KeyCode::PageUp => {
            app.chat_scroll_active = true;
            app.chat_scroll_offset = app.chat_scroll_offset.saturating_add(10);
        }
        KeyCode::PageDown => {
            if app.chat_scroll_offset == 0 {
                app.chat_scroll_active = false;
            } else {
                app.chat_scroll_offset = app.chat_scroll_offset.saturating_sub(10);
            }
        }
        _ => {}
    }
}

fn amend_pending_confirmation(app: &mut App) {
    let Some(confirm) = app.pending_confirmation.as_ref() else {
        return;
    };

    let replacement = pending_confirmation_amend_text(confirm);
    app.input.set_text(&replacement);
    if let Some(tx) = &app.confirm_tx {
        let _ = tx.send(ConfirmResponse::Deny);
    }
    app.pending_confirmation = None;
}

fn pending_confirmation_amend_text(confirm: &crate::app::PendingConfirmation) -> String {
    let parsed: serde_json::Value =
        serde_json::from_str(&confirm.arguments).unwrap_or(serde_json::Value::Null);

    match confirm.name.as_str() {
        "bash" | "powershell" => parsed
            .get("command")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        "read_file" | "write_file" | "edit_file" | "multi_edit" => parsed
            .get("file_path")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        _ => confirm.arguments.clone(),
    }
}

fn explain_pending_confirmation(app: &mut App) {
    let Some(confirm) = app.pending_confirmation.as_ref() else {
        return;
    };

    let explanation = match confirm.name.as_str() {
        "bash" | "powershell" => {
            "This command requires approval because shell access can run arbitrary programs, read local files, or modify the workspace.".to_string()
        }
        "edit_file" | "write_file" | "multi_edit" => {
            "This command requires approval because it will change files in the workspace.".to_string()
        }
        "web_search" | "web_fetch" | "web_browser" => {
            "This command requires approval because it uses network access.".to_string()
        }
        _ => format!(
            "{} requires approval before execution.",
            confirm.name
        ),
    };

    push_system_entry(app, explanation);
}

fn execute_inspector_internal_action(app: &mut App, command: &str) -> bool {
    match command {
        INSPECTOR_CONFIRM_ALLOW => {
            if let Some(tx) = &app.confirm_tx {
                let _ = tx.send(ConfirmResponse::Allow);
            }
            app.pending_confirmation = None;
            true
        }
        INSPECTOR_CONFIRM_ALWAYS => {
            if let Some(ref confirm) = app.pending_confirmation {
                if !app.session.always_allow_tools.contains(&confirm.name) {
                    app.session.always_allow_tools.push(confirm.name.clone());
                }
            }
            if let Some(tx) = &app.confirm_tx {
                let _ = tx.send(ConfirmResponse::Allow);
            }
            app.pending_confirmation = None;
            true
        }
        INSPECTOR_CONFIRM_DENY => {
            if let Some(tx) = &app.confirm_tx {
                let _ = tx.send(ConfirmResponse::Deny);
            }
            app.pending_confirmation = None;
            true
        }
        _ => false,
    }
}

fn inspector_action_fallback_detail(effect: &InspectorActionEffect) -> Option<&'static str> {
    match effect {
        InspectorActionEffect::OpenArtifact { .. } => Some("fallback ran command: open artifact"),
        InspectorActionEffect::OpenInspectorTarget { .. } => {
            Some("fallback ran command: inspect target")
        }
        _ => None,
    }
}

fn effect_from_target(target: &str, command: &str) -> InspectorActionEffect {
    if command.trim_start().starts_with("/inspect artifact") {
        InspectorActionEffect::OpenArtifact {
            target: target.to_string(),
            command: command.to_string(),
        }
    } else {
        InspectorActionEffect::OpenInspectorTarget {
            target: target.to_string(),
            command: command.to_string(),
        }
    }
}

fn execute_inspector_open_action(
    app: &mut App,
    effect: &InspectorActionEffect,
    engine: &Arc<Mutex<AgentEngine>>,
    tools: &Arc<ToolRegistry>,
    engine_event_tx: &mpsc::UnboundedSender<EngineEvent>,
) -> Result<crate::ui::inspector::InspectorDocument, String> {
    let (command, args, title) = inspector_open_command(effect)?;

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
            streaming_markdown_stable_len: &mut app.streaming_markdown_stable_len,
            streaming_markdown_cached_buf_len: &mut app.streaming_markdown_cached_buf_len,
            streaming_markdown_cached_width: &mut app.streaming_markdown_cached_width,
            streaming_markdown_preview_source: &mut app.streaming_markdown_preview_source,
            streaming_markdown_preview: &mut app.streaming_markdown_preview,
            streaming_markdown_remainder: &mut app.streaming_markdown_remainder,
            tools,
            session: &mut app.session,
            input: &mut app.input,
            terminal_caps: &app.terminal_caps,
            input_history: app.history.entries(),
            should_quit: &mut app.should_quit,
            session_start: app.session_start,
            turn_started_at: app.turn_started_at,
            cmd_registry: &app.cmd_registry,
            engine_event_tx,
        };
        app.cmd_registry.execute_command(command, &args, &mut ctx)
    };

    match result {
        Some(Ok(CommandOutput::OpenInspector(document))) => Ok(document),
        Some(Ok(CommandOutput::Message(body))) => Ok(document_from_command_output(
            &title,
            body.lines().map(str::to_string).collect(),
        )),
        Some(Ok(CommandOutput::Messages(lines))) => Ok(document_from_command_output(&title, lines)),
        Some(Ok(CommandOutput::Silent)) => {
            Err("typed inspector target produced no output".to_string())
        }
        Some(Ok(CommandOutput::StartWizard(_)) | Ok(CommandOutput::ReloadProvider { .. })) => {
            Err("typed inspector target is not viewable".to_string())
        }
        Some(Err(reason)) => Err(reason),
        None => Err("inspect command unavailable".to_string()),
    }
}

fn inspector_open_command(
    effect: &InspectorActionEffect,
) -> Result<(&'static str, String, String), String> {
    Ok(match effect {
        InspectorActionEffect::OpenArtifact { target, .. } => {
            let target = target.trim();
            if target.is_empty() {
                return Err("typed artifact action has no target".to_string());
            }
            (
                "inspect",
                format!("artifact {}", target),
                "Artifact inspector".to_string(),
            )
        }
        InspectorActionEffect::OpenInspectorTarget { target, .. } => {
            let target = target.trim();
            if target.is_empty() {
                return Err("typed inspector action has no target".to_string());
            }
            (
                "inspect",
                target.to_string(),
                format!("{} inspector", target),
            )
        }
        _ => return Err("not an inspector open action".to_string()),
    })
}

fn execute_inspector_typed_action(
    app: &mut App,
    effect: &InspectorActionEffect,
) -> Result<(), String> {
    match effect {
        InspectorActionEffect::InternalConfirmAllow => {
            execute_typed_confirmation(app, ConfirmResponse::Allow, false)
        }
        InspectorActionEffect::InternalConfirmAlways => {
            execute_typed_confirmation(app, ConfirmResponse::Allow, true)
        }
        InspectorActionEffect::InternalConfirmDeny => {
            execute_typed_confirmation(app, ConfirmResponse::Deny, false)
        }
        _ => Err("not an internal confirmation action".to_string()),
    }
}

fn execute_typed_confirmation(
    app: &mut App,
    response: ConfirmResponse,
    remember_tool: bool,
) -> Result<(), String> {
    let Some(confirm) = app.pending_confirmation.as_ref() else {
        return Err("no pending confirmation".to_string());
    };
    let Some(tx) = &app.confirm_tx else {
        return Err("confirmation channel unavailable".to_string());
    };
    if remember_tool && !app.session.always_allow_tools.contains(&confirm.name) {
        app.session.always_allow_tools.push(confirm.name.clone());
    }
    tx.send(response)
        .map_err(|_| "confirmation channel closed".to_string())?;
    app.pending_confirmation = None;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use tokio::sync::mpsc;
    use yode_core::engine::ConfirmResponse;
    use yode_llm::registry::ProviderRegistry;
    use yode_tools::registry::ToolRegistry;

    use crate::app::{App, PendingConfirmation};
    use crate::ui::inspector::InspectorActionEffect;

    use super::{
        execute_inspector_internal_action, execute_inspector_typed_action,
        inspector_action_fallback_detail, inspector_open_command, INSPECTOR_CONFIRM_ALLOW,
        INSPECTOR_CONFIRM_ALWAYS, INSPECTOR_CONFIRM_DENY,
    };

    fn test_app() -> App {
        App::new(
            "test-model".to_string(),
            "session-1234".to_string(),
            "/tmp".to_string(),
            "test".to_string(),
            Vec::new(),
            HashMap::new(),
            Arc::new(ProviderRegistry::new()),
            Arc::new(ToolRegistry::new()),
        )
    }

    #[tokio::test]
    async fn inspector_internal_actions_resolve_pending_confirmation() {
        let mut app = test_app();
        let (tx, mut rx) = mpsc::unbounded_channel::<ConfirmResponse>();
        app.confirm_tx = Some(tx);
        app.pending_confirmation = Some(PendingConfirmation {
            id: "a".to_string(),
            name: "bash".to_string(),
            arguments: "{\"command\":\"echo hi\"}".to_string(),
        });

        assert!(execute_inspector_internal_action(
            &mut app,
            INSPECTOR_CONFIRM_ALLOW
        ));
        assert!(matches!(rx.recv().await, Some(ConfirmResponse::Allow)));
        assert!(app.pending_confirmation.is_none());
    }

    #[tokio::test]
    async fn inspector_internal_action_can_always_allow() {
        let mut app = test_app();
        let (tx, mut rx) = mpsc::unbounded_channel::<ConfirmResponse>();
        app.confirm_tx = Some(tx);
        app.pending_confirmation = Some(PendingConfirmation {
            id: "a".to_string(),
            name: "bash".to_string(),
            arguments: "{\"command\":\"echo hi\"}".to_string(),
        });

        assert!(execute_inspector_internal_action(
            &mut app,
            INSPECTOR_CONFIRM_ALWAYS
        ));
        assert!(matches!(rx.recv().await, Some(ConfirmResponse::Allow)));
        assert!(app.session.always_allow_tools.contains(&"bash".to_string()));
    }

    #[tokio::test]
    async fn inspector_internal_action_can_deny() {
        let mut app = test_app();
        let (tx, mut rx) = mpsc::unbounded_channel::<ConfirmResponse>();
        app.confirm_tx = Some(tx);
        app.pending_confirmation = Some(PendingConfirmation {
            id: "a".to_string(),
            name: "bash".to_string(),
            arguments: "{\"command\":\"echo hi\"}".to_string(),
        });

        assert!(execute_inspector_internal_action(
            &mut app,
            INSPECTOR_CONFIRM_DENY
        ));
        assert!(matches!(rx.recv().await, Some(ConfirmResponse::Deny)));
        assert!(app.pending_confirmation.is_none());
    }

    #[tokio::test]
    async fn inspector_typed_permission_actions_resolve_allow_always_and_deny() {
        let cases = [
            (
                InspectorActionEffect::InternalConfirmAllow,
                ConfirmResponse::Allow,
                false,
            ),
            (
                InspectorActionEffect::InternalConfirmAlways,
                ConfirmResponse::Allow,
                true,
            ),
            (
                InspectorActionEffect::InternalConfirmDeny,
                ConfirmResponse::Deny,
                false,
            ),
        ];

        for (effect, expected, should_remember) in cases {
            let mut app = test_app();
            let (tx, mut rx) = mpsc::unbounded_channel::<ConfirmResponse>();
            app.confirm_tx = Some(tx);
            app.pending_confirmation = Some(PendingConfirmation {
                id: "a".to_string(),
                name: "bash".to_string(),
                arguments: "{\"command\":\"echo hi\"}".to_string(),
            });

            execute_inspector_typed_action(&mut app, &effect).unwrap();
            let actual = rx.recv().await;
            match expected {
                ConfirmResponse::Allow => assert!(matches!(actual, Some(ConfirmResponse::Allow))),
                ConfirmResponse::Deny => assert!(matches!(actual, Some(ConfirmResponse::Deny))),
            }
            assert!(app.pending_confirmation.is_none());
            assert_eq!(
                app.session.always_allow_tools.contains(&"bash".to_string()),
                should_remember
            );
        }
    }

    #[test]
    fn inspector_typed_permission_action_reports_missing_pending_confirmation() {
        let mut app = test_app();
        let error =
            execute_inspector_typed_action(&mut app, &InspectorActionEffect::InternalConfirmAllow)
                .unwrap_err();
        assert_eq!(error, "no pending confirmation");
    }

    #[test]
    fn typed_open_actions_report_command_handoff_fallback() {
        assert_eq!(
            inspector_action_fallback_detail(&InspectorActionEffect::OpenArtifact {
                target: "bundle".to_string(),
                command: "/inspect artifact bundle".to_string(),
            }),
            Some("fallback ran command: open artifact")
        );
        assert_eq!(
            inspector_action_fallback_detail(&InspectorActionEffect::OpenInspectorTarget {
                target: "diagnostics".to_string(),
                command: "/diagnostics".to_string(),
            }),
            Some("fallback ran command: inspect target")
        );
        assert_eq!(
            inspector_action_fallback_detail(&InspectorActionEffect::RunCommand(
                "/help".to_string()
            )),
            None
        );
    }

    #[test]
    fn typed_open_artifact_builds_inspect_command_or_reports_missing_target() {
        assert_eq!(
            inspector_open_command(&InspectorActionEffect::OpenArtifact {
                target: "bundle".to_string(),
                command: "/inspect artifact bundle".to_string(),
            })
            .unwrap(),
            (
                "inspect",
                "artifact bundle".to_string(),
                "Artifact inspector".to_string()
            )
        );

        let error = inspector_open_command(&InspectorActionEffect::OpenArtifact {
            target: " ".to_string(),
            command: "/inspect artifact ".to_string(),
        })
        .unwrap_err();
        assert_eq!(error, "typed artifact action has no target");
    }

    #[test]
    fn typed_open_inspector_target_builds_inspect_command_or_reports_missing_target() {
        assert_eq!(
            inspector_open_command(&InspectorActionEffect::OpenInspectorTarget {
                target: "status".to_string(),
                command: "/status".to_string(),
            })
            .unwrap(),
            (
                "inspect",
                "status".to_string(),
                "status inspector".to_string()
            )
        );
        assert_eq!(
            inspector_open_command(&InspectorActionEffect::OpenInspectorTarget {
                target: "tasks monitor".to_string(),
                command: "/tasks monitor".to_string(),
            })
            .unwrap(),
            (
                "inspect",
                "tasks monitor".to_string(),
                "tasks monitor inspector".to_string()
            )
        );
        assert_eq!(
            inspector_open_command(&InspectorActionEffect::OpenInspectorTarget {
                target: "reviews latest".to_string(),
                command: "/reviews latest".to_string(),
            })
            .unwrap(),
            (
                "inspect",
                "reviews latest".to_string(),
                "reviews latest inspector".to_string()
            )
        );
        assert_eq!(
            inspector_open_command(&InspectorActionEffect::OpenInspectorTarget {
                target: "teams monitor team-demo".to_string(),
                command: "/teams monitor team-demo".to_string(),
            })
            .unwrap(),
            (
                "inspect",
                "teams monitor team-demo".to_string(),
                "teams monitor team-demo inspector".to_string()
            )
        );
        assert_eq!(
            inspector_open_command(&InspectorActionEffect::OpenInspectorTarget {
                target: "context".to_string(),
                command: "/context".to_string(),
            })
            .unwrap(),
            (
                "inspect",
                "context".to_string(),
                "context inspector".to_string()
            )
        );
        assert_eq!(
            inspector_open_command(&InspectorActionEffect::OpenInspectorTarget {
                target: "keys".to_string(),
                command: "/keybindings".to_string(),
            })
            .unwrap(),
            ("inspect", "keys".to_string(), "keys inspector".to_string())
        );
        assert_eq!(
            inspector_open_command(&InspectorActionEffect::OpenInspectorTarget {
                target: "history search build".to_string(),
                command: "/history search build".to_string(),
            })
            .unwrap(),
            (
                "inspect",
                "history search build".to_string(),
                "history search build inspector".to_string()
            )
        );
        assert_eq!(
            inspector_open_command(&InspectorActionEffect::OpenInspectorTarget {
                target: "update status".to_string(),
                command: "/update status".to_string(),
            })
            .unwrap(),
            (
                "inspect",
                "update status".to_string(),
                "update status inspector".to_string()
            )
        );
        assert_eq!(
            inspector_open_command(&InspectorActionEffect::OpenInspectorTarget {
                target: "memory compare latest latest-1".to_string(),
                command: "/memory compare latest latest-1".to_string(),
            })
            .unwrap(),
            (
                "inspect",
                "memory compare latest latest-1".to_string(),
                "memory compare latest latest-1 inspector".to_string()
            )
        );
        assert_eq!(
            inspector_open_command(&InspectorActionEffect::OpenInspectorTarget {
                target: "workflows preview latest".to_string(),
                command: "/workflows preview latest".to_string(),
            })
            .unwrap(),
            (
                "inspect",
                "workflows preview latest".to_string(),
                "workflows preview latest inspector".to_string()
            )
        );
        assert_eq!(
            inspector_open_command(&InspectorActionEffect::OpenInspectorTarget {
                target: "tools verbose".to_string(),
                command: "/tools verbose".to_string(),
            })
            .unwrap(),
            (
                "inspect",
                "tools verbose".to_string(),
                "tools verbose inspector".to_string()
            )
        );
        assert_eq!(
            inspector_open_command(&InspectorActionEffect::OpenInspectorTarget {
                target: "checkpoint diff latest latest-1".to_string(),
                command: "/checkpoint diff latest latest-1".to_string(),
            })
            .unwrap(),
            (
                "inspect",
                "checkpoint diff latest latest-1".to_string(),
                "checkpoint diff latest latest-1 inspector".to_string()
            )
        );
        assert_eq!(
            inspector_open_command(&InspectorActionEffect::OpenInspectorTarget {
                target: "remote-control queue".to_string(),
                command: "/remote-control queue".to_string(),
            })
            .unwrap(),
            (
                "inspect",
                "remote-control queue".to_string(),
                "remote-control queue inspector".to_string()
            )
        );
        assert_eq!(
            inspector_open_command(&InspectorActionEffect::OpenInspectorTarget {
                target: "plugin list".to_string(),
                command: "/plugin list".to_string(),
            })
            .unwrap(),
            (
                "inspect",
                "plugin list".to_string(),
                "plugin list inspector".to_string()
            )
        );

        let error = inspector_open_command(&InspectorActionEffect::OpenInspectorTarget {
            target: String::new(),
            command: "/status".to_string(),
        })
        .unwrap_err();
        assert_eq!(error, "typed inspector action has no target");
    }
}
