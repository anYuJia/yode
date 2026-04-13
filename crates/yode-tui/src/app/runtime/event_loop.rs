use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::{mpsc, Mutex};

use yode_core::engine::{AgentEngine, EngineEvent};
use yode_tools::registry::ToolRegistry;

use crate::event::{self, AppEvent};
use crate::ui;

use super::super::engine_events::handle_engine_event;
use super::super::key_dispatch::handle_key_event;
use super::super::scrollback::flush_entries_to_scrollback;
use super::super::{push_grouped_system_entry, App, ChatEntry, ChatRole};

pub(super) async fn run_app(
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
        maybe_surface_runtime_task_notifications(app, &engine);

        crossterm::execute!(
            terminal.backend_mut(),
            crossterm::terminal::BeginSynchronizedUpdate
        )?;

        flush_entries_to_scrollback(terminal, app)?;
        resize_inline_viewport(terminal, app)?;

        terminal.draw(|frame| {
            ui::render(frame, app);
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
                    handle_paste_event(app, text);
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

fn maybe_surface_runtime_task_notifications(app: &mut App, engine: &Arc<Mutex<AgentEngine>>) {
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
                        task.last_progress
                            .as_ref()
                            .map(|progress| format!(" — {}", progress))
                            .unwrap_or_default()
                    ));
                }
                push_grouped_system_entry(app, "Background tasks still running", lines.join("\n"));
                app.last_task_brief_time = Instant::now();
            }
        }
    }
}

fn resize_inline_viewport(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &App,
) -> Result<()> {
    let needed = viewport_height(app, terminal);
    let area = terminal.get_frame().area();
    if area.height == needed {
        return Ok(());
    }

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
                crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine)
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
    Ok(())
}

fn viewport_height(app: &App, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> u16 {
    if let Some(inspector) = app.inspector.as_ref() {
        let total = inspector
            .document
            .active_panel()
            .map(|panel| panel.lines.len() as u16)
            .unwrap_or(1);
        return total.min(16).max(6);
    }
    if let Some(wizard) = app.wizard.as_ref() {
        return wizard.viewport_height() + 1;
    }
    if app.pending_confirmation.is_some() {
        return 4;
    }

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
}

fn handle_paste_event(app: &mut App, text: String) {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    if let Some(wizard) = app.wizard.as_mut() {
        for ch in text.chars() {
            if ch != '\n' && ch != '\r' {
                wizard.input_char(ch);
            }
        }
    } else if super::super::input::should_fold_paste(&text) {
        app.input.insert_attachment(text);
    } else {
        for line in text.split_inclusive('\n') {
            let clean = line.trim_end_matches('\n');
            for ch in clean.chars() {
                app.input.insert_char(ch);
            }
            if line.ends_with('\n') {
                app.input.insert_newline();
            }
        }
    }
}
