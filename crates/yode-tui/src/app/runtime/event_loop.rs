use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::{mpsc, Mutex};

use yode_core::engine::{AgentEngine, EngineEvent};
use yode_tools::registry::ToolRegistry;
use yode_tools::RuntimeTaskNotification;

use crate::event::{self, AppEvent};
use crate::ui;
use crate::ui::layout::status_area_height;

use super::super::engine_events::handle_engine_event;
use super::super::key_dispatch::handle_key_event;
use super::super::scrollback::flush_entries_to_scrollback;
use super::super::{push_system_entry, App};

pub(super) async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    engine: Arc<Mutex<AgentEngine>>,
    tools: Arc<ToolRegistry>,
    engine_event_tx: mpsc::UnboundedSender<EngineEvent>,
    engine_event_rx: &mut mpsc::UnboundedReceiver<EngineEvent>,
) -> Result<()> {
    let mut force_redraw = true;
    loop {
        app.sync_thinking();
        let mut ui_dirty = false;

        if matches!(app.turn_status, crate::app::TurnStatus::Done { .. })
            && app
                .turn_done_at
                .is_some_and(|done_at| done_at.elapsed() >= Duration::from_secs(3))
        {
            app.turn_status = crate::app::TurnStatus::Idle;
            app.turn_done_at = None;
            ui_dirty = true;
        }

        while let Ok(event) = engine_event_rx.try_recv() {
            handle_engine_event(app, event, &engine, &engine_event_tx);
            ui_dirty = true;
        }
        if maybe_surface_runtime_task_notifications(app, &engine) {
            ui_dirty = true;
        }

        if force_redraw || ui_dirty {
            resize_inline_viewport(terminal, app)?;
        }

        let scrollback_dirty = flush_entries_to_scrollback(terminal, app)?;
        if force_redraw || ui_dirty || scrollback_dirty {
            crossterm::execute!(
                terminal.backend_mut(),
                crossterm::terminal::BeginSynchronizedUpdate
            )?;

            terminal.draw(|frame| {
                ui::render(frame, app);
            })?;

            crossterm::execute!(
                terminal.backend_mut(),
                crossterm::terminal::EndSynchronizedUpdate
            )?;
            force_redraw = false;
        }

        if app.should_quit {
            break;
        }

        if let Some(app_event) = event::poll_event(Duration::from_millis(50))? {
            match app_event {
                AppEvent::Key(key) => {
                    handle_key_event(terminal, app, key, &engine, &tools, &engine_event_tx);
                    force_redraw = true;
                }
                AppEvent::Paste(text) => {
                    handle_paste_event(app, text);
                    force_redraw = true;
                }
                AppEvent::Resize(_, _) => {
                    force_redraw = true;
                }
                AppEvent::Tick => {
                    if app.is_thinking && app.thinking.advance_spinner() {
                        force_redraw = true;
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

fn maybe_surface_runtime_task_notifications(
    app: &mut App,
    engine: &Arc<Mutex<AgentEngine>>,
) -> bool {
    if should_defer_runtime_task_notifications(app) {
        return false;
    }

    let mut changed = false;
    if let Ok(engine_guard) = engine.try_lock() {
        for notification in engine_guard.drain_runtime_task_notifications() {
            push_system_entry(app, render_task_notification_xml(&notification));
            changed = true;
        }

        if app.last_task_brief_time.elapsed() >= Duration::from_secs(60) {
            let running = engine_guard
                .runtime_tasks_snapshot()
                .into_iter()
                .filter(|task| matches!(task.status, yode_tools::RuntimeTaskStatus::Running))
                .collect::<Vec<_>>();
            if !running.is_empty() {
                let lines = background_task_brief_lines(&running);
                push_system_entry(app, lines.join("\n"));
                app.last_task_brief_time = Instant::now();
                changed = true;
            }
        }
    }
    changed
}

fn should_defer_runtime_task_notifications(app: &App) -> bool {
    app.is_processing || !app.streaming_buf.is_empty() || !app.streaming_markdown_preview.is_empty()
}

fn render_task_notification_xml(notification: &RuntimeTaskNotification) -> String {
    let status = match notification.status {
        yode_tools::RuntimeTaskStatus::Completed => "completed",
        yode_tools::RuntimeTaskStatus::Failed => "failed",
        yode_tools::RuntimeTaskStatus::Cancelled => "killed",
        yode_tools::RuntimeTaskStatus::Pending => "pending",
        yode_tools::RuntimeTaskStatus::Running => "running",
    };
    let mut output = String::new();
    output.push_str("<task-notification>\n");
    output.push_str(&format!(
        "<task-id>{}</task-id>\n",
        xml_escape(&notification.task_id)
    ));
    output.push_str(&format!("<status>{}</status>\n", status));
    output.push_str(&format!(
        "<summary>{}</summary>\n",
        xml_escape(notification_summary(notification))
    ));
    if let Some(path) = notification.output_path.as_deref() {
        output.push_str(&format!(
            "<output-path>{}</output-path>\n",
            xml_escape(path)
        ));
    }
    if let Some(path) = notification.transcript_path.as_deref() {
        output.push_str(&format!(
            "<transcript-path>{}</transcript-path>\n",
            xml_escape(path)
        ));
    }
    if let Some(result) = notification.result_preview.as_deref() {
        output.push_str("<result>");
        output.push_str(&xml_escape(result));
        output.push_str("</result>\n");
    }
    if notification.duration_ms.is_some() || notification.tool_uses.is_some() {
        output.push_str("<usage>\n");
        if let Some(duration_ms) = notification.duration_ms {
            output.push_str(&format!("  <duration_ms>{}</duration_ms>\n", duration_ms));
        }
        if let Some(tool_uses) = notification.tool_uses {
            output.push_str(&format!("  <tool_uses>{}</tool_uses>\n", tool_uses));
        }
        output.push_str("</usage>\n");
    }
    output.push_str("</task-notification>");
    output
}

fn notification_summary(notification: &RuntimeTaskNotification) -> &str {
    if notification.summary.trim().is_empty() {
        &notification.message
    } else {
        &notification.summary
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn background_task_brief_lines(tasks: &[yode_tools::RuntimeTask]) -> Vec<String> {
    let mut lines = vec![format!(
        "Background tasks still running · {} active",
        tasks.len()
    )];
    for task in tasks.iter().take(2) {
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
    if tasks.len() > 2 {
        lines.push(format!("  - … +{} more tasks", tasks.len() - 2));
    }
    lines
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use yode_llm::registry::ProviderRegistry;
    use yode_tools::registry::ToolRegistry;

    use crate::app::{App, InspectorView};
    use crate::ui::inspector::InspectorDocument;

    use super::{
        background_task_brief_lines, inline_viewport_target, inspector_viewport_height,
        render_task_notification_xml, should_anchor_inline_to_bottom,
        should_defer_runtime_task_notifications,
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

    #[test]
    fn background_task_brief_lines_are_compact() {
        let tasks = (0..3)
            .map(|index| yode_tools::RuntimeTask {
                id: format!("task-{}", index),
                kind: "agent".to_string(),
                source_tool: "agent".to_string(),
                description: format!("desc {}", index),
                status: yode_tools::RuntimeTaskStatus::Running,
                attempt: 1,
                retry_of: None,
                output_path: format!("/tmp/task-{}.log", index),
                transcript_path: None,
                created_at: "2026-01-01 00:00:00".to_string(),
                started_at: None,
                completed_at: None,
                last_progress: Some("building".to_string()),
                last_progress_at: None,
                progress_history: Vec::new(),
                error: None,
            })
            .collect::<Vec<_>>();

        let lines = background_task_brief_lines(&tasks);
        assert!(lines[0].contains("3 active"));
        assert!(lines[1].contains("task-0 [agent] desc 0"));
        assert!(lines[3].contains("+1 more tasks"));
    }

    #[test]
    fn task_notification_renders_claude_compatible_xml() {
        let notification = yode_tools::RuntimeTaskNotification {
            task_id: "task-1".to_string(),
            task_kind: "agent".to_string(),
            status: yode_tools::RuntimeTaskStatus::Completed,
            severity: yode_tools::RuntimeTaskNotificationSeverity::Success,
            message: "Task task-1 completed".to_string(),
            summary: "completed".to_string(),
            result_preview: Some("fixed <bug>".to_string()),
            output_path: Some("/tmp/out".to_string()),
            transcript_path: Some("/tmp/transcript".to_string()),
            duration_ms: Some(12),
            tool_uses: Some(3),
        };

        let rendered = render_task_notification_xml(&notification);
        assert!(rendered.contains("<task-notification>"));
        assert!(rendered.contains("<task-id>task-1</task-id>"));
        assert!(rendered.contains("<status>completed</status>"));
        assert!(rendered.contains("<result>fixed &lt;bug&gt;</result>"));
        assert!(rendered.contains("<duration_ms>12</duration_ms>"));
        assert!(rendered.contains("<tool_uses>3</tool_uses>"));
    }

    #[test]
    fn runtime_task_notifications_wait_until_turn_finishes() {
        let mut app = test_app();
        assert!(!should_defer_runtime_task_notifications(&app));

        app.is_processing = true;
        assert!(should_defer_runtime_task_notifications(&app));

        app.is_processing = false;
        app.streaming_buf = "partial response".to_string();
        assert!(should_defer_runtime_task_notifications(&app));

        app.streaming_buf.clear();
        app.streaming_markdown_preview = vec![ratatui::text::Line::from("preview")];
        assert!(should_defer_runtime_task_notifications(&app));
    }

    #[test]
    fn inspector_viewport_reserves_status_line() {
        let mut app = test_app();
        app.inspector.views.push(InspectorView {
            document: InspectorDocument::single(
                "Tool",
                (0..20).map(|index| format!("line {}", index)).collect(),
            ),
        });

        assert_eq!(inspector_viewport_height(&app), Some(17));
    }

    #[test]
    fn inline_viewport_target_keeps_empty_session_compact() {
        assert_eq!(inline_viewport_target(8, 4, 24, 7, false), (7, 8));
        assert_eq!(inline_viewport_target(20, 4, 24, 7, false), (7, 17));
        assert_eq!(inline_viewport_target(0, 0, 0, 7, false), (1, 0));
    }

    #[test]
    fn inline_viewport_target_anchors_active_session_to_terminal_bottom() {
        assert_eq!(inline_viewport_target(8, 4, 24, 7, true), (7, 17));
        assert_eq!(inline_viewport_target(8, 4, 24, 40, true), (24, 0));
        assert_eq!(inline_viewport_target(0, 0, 0, 7, true), (1, 0));
    }

    #[test]
    fn hidden_or_restored_entries_do_not_force_bottom_anchor() {
        let mut app = test_app();
        app.chat_entries.push(crate::app::ChatEntry::new(
            crate::app::ChatRole::System,
            "Session resumed.".to_string(),
        ));
        app.printed_count = 1;

        assert!(!should_anchor_inline_to_bottom(&app));

        app.is_processing = true;
        assert!(should_anchor_inline_to_bottom(&app));
    }
}

fn resize_inline_viewport(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &App,
) -> Result<()> {
    let needed = viewport_height(app, terminal);
    let area = terminal.get_frame().area();
    let (_, terminal_height) = crossterm::terminal::size()?;
    let (needed, new_y) = inline_viewport_target(
        area.y,
        area.height,
        terminal_height,
        needed,
        should_anchor_inline_to_bottom(app),
    );
    if area.height == needed && area.y == new_y {
        return Ok(());
    }

    if new_y < area.y {
        let grow_by = area.y - new_y;
        crossterm::execute!(
            terminal.backend_mut(),
            crossterm::terminal::ScrollUp(grow_by)
        )?;
    } else {
        for row in area.y..new_y {
            crossterm::execute!(
                terminal.backend_mut(),
                crossterm::cursor::MoveTo(0, row),
                crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine)
            )?;
        }
    }
    let new_area = ratatui::layout::Rect {
        x: area.x,
        y: new_y,
        width: area.width,
        height: needed,
    };
    terminal.viewport = ratatui::Viewport::Inline(needed);
    terminal.set_viewport_area(new_area);
    terminal.clear()?;
    Ok(())
}

fn inline_viewport_target(
    current_y: u16,
    current_height: u16,
    terminal_height: u16,
    needed: u16,
    anchor_to_bottom: bool,
) -> (u16, u16) {
    let height = needed.min(terminal_height.max(1));
    if anchor_to_bottom {
        return (height, terminal_height.saturating_sub(height));
    }

    let current_bottom = current_y.saturating_add(current_height);
    let target_y = if current_bottom >= terminal_height {
        terminal_height.saturating_sub(height)
    } else {
        current_y.min(terminal_height.saturating_sub(height))
    };
    (height, target_y)
}

fn should_anchor_inline_to_bottom(app: &App) -> bool {
    app.is_processing
        || app.is_thinking
        || app.turn_status.is_visible()
        || !app.turn_completion.is_empty()
        || !app.streaming_buf.is_empty()
        || !app.streaming_markdown_preview.is_empty()
}

fn viewport_height(app: &App, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> u16 {
    if let Some(height) = inspector_viewport_height(app) {
        return height;
    }
    if let Some(wizard) = app.wizard.as_ref() {
        return wizard.viewport_height() + 1;
    }
    if app.pending_confirmation.is_some() {
        return crate::ui::tool_confirm::INLINE_CONFIRM_VIEWPORT_HEIGHT;
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
    let thinking_line = status_area_height(app, completion_lines);
    let pending_line = app.pending_inputs.len() as u16;
    visual_lines.clamp(1, 5) + completion_lines + thinking_line + pending_line + 4
}

fn inspector_viewport_height(app: &App) -> Option<u16> {
    let inspector = app.inspector.views.last()?;
    let total = inspector
        .document
        .active_panel()
        .map(|panel| panel.lines.len() as u16)
        .unwrap_or(1);
    Some(total.clamp(6, 16) + crate::ui::INSPECTOR_STATUS_HEIGHT)
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
