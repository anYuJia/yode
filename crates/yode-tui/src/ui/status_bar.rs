use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::time::Duration;

use super::badges::{
    budget_badge_label, permission_mode_badge, queue_badge_label, task_badge_label,
};
use super::palette::{LIGHT, MUTED, SEP};
use super::responsive::{density_from_width, status_section_mode, Density, StatusSectionMode};
use super::status_summary::{
    compaction_badge, context_badge, cost_badge, memory_badge, prompt_cache_badge, push_badge,
    runtime_family_badges, runtime_status_snapshot,
};
use crate::app::{App, TurnStatus};
use yode_core::cost_tracker::estimate_token_cost;

/// Top separator line: ────────────────────────────
pub fn render_separator(frame: &mut Frame, area: Rect) {
    let line = Line::from(Span::styled(
        "─".repeat(area.width as usize),
        Style::default().fg(SEP),
    ));
    frame.render_widget(Paragraph::new(line), area);
}

/// Bottom info line with session details:
///   ⚡ auto · done 1m26s · time 4m02s · model sonnet · 24.0k↑ 74↓ tok · /help
pub fn render_info_line(frame: &mut Frame, area: Rect, app: &App) {
    frame.render_widget(
        Paragraph::new(Line::from(composer_status_spans(app, area.width))),
        area,
    );
}

fn composer_status_spans(app: &App, width: u16) -> Vec<Span<'static>> {
    let density = density_from_width(width, 68, 96);
    let section_mode = status_section_mode(width);
    let snapshot = runtime_status_snapshot(app);
    let mut parts: Vec<Span> = Vec::new();

    // Prefix
    parts.push(Span::styled(
        if matches!(density, Density::Narrow) {
            " "
        } else {
            "  "
        },
        Style::default(),
    ));

    // Permission mode badge
    let (mode_text, mode_color) = permission_mode_badge(app.session.permission_mode, density);
    parts.push(Span::styled(mode_text, Style::default().fg(mode_color)));
    parts.push(Span::styled("· ", Style::default().fg(SEP)));

    parts.push(Span::styled(
        turn_state_label(app, density),
        turn_state_style(app),
    ));
    parts.push(Span::styled("· ", Style::default().fg(SEP)));
    parts.push(Span::styled(
        session_elapsed_label(app.session_start.elapsed(), density),
        Style::default().fg(LIGHT),
    ));
    parts.push(Span::styled("· ", Style::default().fg(SEP)));

    if !matches!(section_mode, StatusSectionMode::Collapsed) {
        parts.push(Span::styled(
            model_status_label(&app.session.model, density),
            Style::default().fg(MUTED),
        ));
        parts.push(Span::styled("· ", Style::default().fg(SEP)));
    }

    // Token count (input↑ output↓)
    let input_prefix = if app.session.input_estimated { "~" } else { "" };
    parts.push(Span::styled(
        match density {
            Density::Wide => format!(
                "{}{}↑ {}↓ tok ",
                input_prefix,
                format_token_count(app.session.input_tokens),
                format_token_count(app.session.output_tokens)
            ),
            Density::Medium => format!(
                "{}{}↑ {}↓ ",
                input_prefix,
                format_token_count(app.session.input_tokens),
                format_token_count(app.session.output_tokens)
            ),
            Density::Narrow => format!(
                "{}{}↑{}↓ ",
                input_prefix,
                format_token_count(app.session.input_tokens),
                format_token_count(app.session.output_tokens)
            ),
        },
        Style::default().fg(LIGHT),
    ));
    parts.push(Span::styled("· ", Style::default().fg(SEP)));

    let runtime_cost_badge = cost_badge(snapshot.state.as_ref(), density);
    if runtime_cost_badge.is_none() {
        if let Some(cost_label) = session_estimated_cost_label(app, density) {
            parts.push(Span::styled(
                cost_label,
                Style::default().fg(Color::LightCyan),
            ));
            parts.push(Span::styled("· ", Style::default().fg(SEP)));
        }
    }

    if let Some(turn_tokens) = turn_token_label(app, density) {
        parts.push(Span::styled(turn_tokens, Style::default().fg(MUTED)));
        parts.push(Span::styled("· ", Style::default().fg(SEP)));
    }

    // Tool calls (Claude Code-style session total)
    if app.session.tool_call_count > 0 {
        parts.push(Span::styled(
            match density {
                Density::Wide => format!("{} tools ", app.session.tool_call_count),
                Density::Medium | Density::Narrow => format!("{}t ", app.session.tool_call_count),
            },
            Style::default().fg(LIGHT),
        ));
        parts.push(Span::styled("· ", Style::default().fg(SEP)));
    }

    let fallback_context_tokens: usize = app
        .chat_entries
        .iter()
        .map(|e| e.content.len())
        .sum::<usize>()
        / 4;
    if !matches!(section_mode, StatusSectionMode::Collapsed) {
        push_badge(
            &mut parts,
            context_badge(snapshot.state.as_ref(), fallback_context_tokens, density),
        );
        if let Some(badge) = compaction_badge(snapshot.state.as_ref(), density) {
            push_badge(&mut parts, badge);
        }
        if let Some(badge) = memory_badge(snapshot.state.as_ref(), density) {
            push_badge(&mut parts, badge);
        }
        if let Some(badge) = prompt_cache_badge(snapshot.state.as_ref(), density) {
            push_badge(&mut parts, badge);
        }
        if let Some(badge) = runtime_cost_badge {
            push_badge(&mut parts, badge);
        }
    }

    // Queue
    if !app.pending_inputs.is_empty() {
        parts.push(Span::styled(
            queue_badge_label(app.pending_inputs.len(), density),
            Style::default().fg(Color::LightMagenta),
        ));
        parts.push(Span::styled("· ", Style::default().fg(SEP)));
    }

    if snapshot.running_tasks > 0 {
        parts.push(Span::styled(
            task_badge_label(snapshot.running_tasks, density),
            Style::default().fg(Color::LightBlue),
        ));
        parts.push(Span::styled("· ", Style::default().fg(SEP)));
    }

    for badge in runtime_family_badges(&snapshot, density) {
        push_badge(&mut parts, badge);
    }

    if let Some(budget_badge) = budget_badge_label(app.turn_tool_count, density) {
        parts.push(Span::styled(
            budget_badge,
            Style::default().fg(Color::LightYellow),
        ));
        parts.push(Span::styled("· ", Style::default().fg(SEP)));
    }

    // Shortcuts hint
    match density {
        Density::Wide => {
            parts.push(Span::styled("Shift+Tab", Style::default().fg(MUTED)));
            parts.push(Span::styled(" · ", Style::default().fg(SEP)));
            parts.push(Span::styled("/help", Style::default().fg(MUTED)));
        }
        Density::Medium => {
            parts.push(Span::styled("S-Tab", Style::default().fg(MUTED)));
            parts.push(Span::styled(" · ", Style::default().fg(SEP)));
            parts.push(Span::styled("/help", Style::default().fg(MUTED)));
        }
        Density::Narrow => {
            parts.push(Span::styled("/h", Style::default().fg(MUTED)));
        }
    }

    parts
}

fn session_elapsed_label(elapsed: Duration, density: Density) -> String {
    let secs = elapsed.as_secs();
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let rem_secs = secs % 60;

    match density {
        Density::Wide => {
            if hours > 0 {
                format!("time {}h{:02}m", hours, mins)
            } else if mins > 0 {
                format!("time {}m{:02}s", mins, rem_secs)
            } else {
                format!("time {}s", rem_secs)
            }
        }
        Density::Medium => {
            if hours > 0 {
                format!("{}h{:02}m", hours, mins)
            } else if mins > 0 {
                format!("{}m{:02}s", mins, rem_secs)
            } else {
                format!("{}s", rem_secs)
            }
        }
        Density::Narrow => {
            if hours > 0 {
                format!("{}h", hours)
            } else if mins > 0 {
                format!("{}m", mins)
            } else {
                format!("{}s", rem_secs)
            }
        }
    }
}

fn turn_state_label(app: &App, density: Density) -> String {
    if app.pending_confirmation.is_some() {
        return match density {
            Density::Wide => "permission ".to_string(),
            Density::Medium => "perm ".to_string(),
            Density::Narrow => "ask ".to_string(),
        };
    }

    match &app.turn_status {
        TurnStatus::Working { .. } => match density {
            Density::Wide => format!("working {} ", compact_duration_label(turn_elapsed(app))),
            Density::Medium => format!("work {} ", compact_duration_label(turn_elapsed(app))),
            Density::Narrow => "run ".to_string(),
        },
        TurnStatus::Done { elapsed, .. } => match density {
            Density::Wide => format!("done {} ", compact_duration_label(*elapsed)),
            Density::Medium => "done ".to_string(),
            Density::Narrow => "ok ".to_string(),
        },
        TurnStatus::Retrying {
            attempt,
            max_attempts,
            ..
        } => match density {
            Density::Wide => format!("retry {}/{} ", attempt, max_attempts),
            Density::Medium => format!("r{}/{} ", attempt, max_attempts),
            Density::Narrow => "retry ".to_string(),
        },
        TurnStatus::Idle => {
            if app.turn_completion.last_turn_message.is_some() {
                match density {
                    Density::Wide | Density::Medium => "done ".to_string(),
                    Density::Narrow => "ok ".to_string(),
                }
            } else {
                match density {
                    Density::Wide | Density::Medium => "idle ".to_string(),
                    Density::Narrow => "idle ".to_string(),
                }
            }
        }
    }
}

fn turn_state_style(app: &App) -> Style {
    if app.pending_confirmation.is_some() {
        return Style::default().fg(Color::Yellow);
    }

    match app.turn_status {
        TurnStatus::Working { .. } => Style::default().fg(Color::LightMagenta),
        TurnStatus::Retrying { .. } => Style::default().fg(Color::Yellow),
        TurnStatus::Done { .. } => Style::default().fg(Color::LightGreen),
        TurnStatus::Idle if app.turn_completion.last_turn_message.is_some() => {
            Style::default().fg(Color::LightGreen)
        }
        TurnStatus::Idle => Style::default().fg(MUTED),
    }
}

fn turn_elapsed(app: &App) -> Duration {
    app.turn_started_at
        .map(|started| started.elapsed())
        .unwrap_or_default()
}

fn turn_token_label(app: &App, density: Density) -> Option<String> {
    if app.session.turn_input_tokens == 0 && app.session.turn_output_tokens == 0 {
        return None;
    }

    Some(match density {
        Density::Wide => format!(
            "turn {}↑ {}↓ ",
            format_token_count(app.session.turn_input_tokens),
            format_token_count(app.session.turn_output_tokens)
        ),
        Density::Medium => format!(
            "t {}↑ {}↓ ",
            format_token_count(app.session.turn_input_tokens),
            format_token_count(app.session.turn_output_tokens)
        ),
        Density::Narrow => format!(
            "{}↑{}↓ ",
            format_token_count(app.session.turn_input_tokens),
            format_token_count(app.session.turn_output_tokens)
        ),
    })
}

fn session_estimated_cost_label(app: &App, density: Density) -> Option<String> {
    if app.session.input_tokens == 0 && app.session.output_tokens == 0 {
        return None;
    }

    let cost = estimate_token_cost(
        &app.session.model,
        app.session.input_tokens.into(),
        app.session.output_tokens.into(),
    );
    if cost <= 0.0 {
        return None;
    }

    Some(match density {
        Density::Wide => format!("cost ${:.4} ", cost),
        Density::Medium => format!("${:.4} ", cost),
        Density::Narrow => format!("${:.3} ", cost),
    })
}

fn model_status_label(model: &str, density: Density) -> String {
    let max_chars = match density {
        Density::Wide => 28,
        Density::Medium => 18,
        Density::Narrow => 0,
    };
    let model = compact_model_name(model, max_chars);
    match density {
        Density::Wide => format!("model {} ", model),
        Density::Medium => format!("{} ", model),
        Density::Narrow => String::new(),
    }
}

fn compact_model_name(model: &str, max_chars: usize) -> String {
    let tail = model.rsplit('/').next().unwrap_or(model);
    let model = tail.strip_prefix("models/").unwrap_or(tail).to_string();
    if max_chars == 0 || model.chars().count() <= max_chars {
        return model;
    }
    let keep = max_chars.saturating_sub(3);
    format!("{}...", model.chars().take(keep).collect::<String>())
}

fn compact_duration_label(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let rem_secs = secs % 60;
    if hours > 0 {
        format!("{}h{:02}m", hours, mins)
    } else if mins > 0 {
        format!("{}m{:02}s", mins, rem_secs)
    } else {
        format!("{}s", rem_secs)
    }
}

fn format_token_count(value: u32) -> String {
    if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}k", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

pub fn render_blank_line(frame: &mut Frame, area: Rect, app: &App) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let mut update_text = String::new();
    let mut update_style = Style::default();

    if let Some(ref version) = app.update.available {
        update_text = format!(" ✨ Update available: {} (restart to apply) ", version);
        update_style = Style::default().fg(Color::LightCyan);
    } else if app.update.downloading {
        update_text = " ⏳ Downloading update... ".to_string();
        update_style = Style::default().fg(Color::Yellow);
    } else if let Some(ref version) = app.update.downloaded {
        update_text = format!(" ✅ Update v{} ready (restart to apply) ", version);
        update_style = Style::default().fg(Color::LightGreen);
    }

    let line = blank_line_parts(area.width, update_text, update_style);

    frame.render_widget(Paragraph::new(line), area);
}

fn blank_line_parts(width: u16, update_text: String, update_style: Style) -> Line<'static> {
    if update_text.is_empty() {
        return Line::from(Span::raw(" ".repeat(width as usize)));
    }

    let update_len = update_text.chars().count();
    let left_padding = width.saturating_sub(update_len as u16) as usize;
    Line::from(vec![
        Span::raw(" ".repeat(left_padding)),
        Span::styled(update_text, update_style),
    ])
}

#[cfg(test)]
mod tests {
    use super::{
        blank_line_parts, compact_model_name, composer_status_spans, session_elapsed_label, Density,
    };
    use crate::app::{App, PendingConfirmation, TurnStatus};
    use crate::ui::badges::task_badge_label;
    use crate::ui::responsive::density_from_width;
    use ratatui::style::Style;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;
    use yode_llm::registry::ProviderRegistry;
    use yode_tools::registry::ToolRegistry;

    fn test_app() -> App {
        App::new(
            "claude-sonnet-4-6-20260422".to_string(),
            "session-1234".to_string(),
            "/tmp".to_string(),
            "test".to_string(),
            Vec::new(),
            HashMap::new(),
            Arc::new(ProviderRegistry::new()),
            Arc::new(ToolRegistry::new()),
        )
    }

    fn spans_to_text(spans: &[ratatui::text::Span<'static>]) -> String {
        spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn status_bar_density_compacts_on_narrow_widths() {
        assert!(matches!(density_from_width(120, 68, 96), Density::Wide));
        assert!(matches!(density_from_width(80, 68, 96), Density::Medium));
        assert!(matches!(density_from_width(50, 68, 96), Density::Narrow));
    }

    #[test]
    fn task_badge_label_compacts_for_small_widths() {
        assert_eq!(task_badge_label(3, Density::Wide), "3 jobs ");
        assert_eq!(task_badge_label(3, Density::Medium), "j3 ");
        assert_eq!(task_badge_label(3, Density::Narrow), "3j ");
    }

    #[test]
    fn bottom_blank_line_is_blank_without_update() {
        let line = blank_line_parts(12, String::new(), Style::default());
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].content.as_ref(), "            ");
    }

    #[test]
    fn bottom_blank_line_right_aligns_update_without_dot_fill() {
        let line = blank_line_parts(10, " update ".to_string(), Style::default());
        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.spans[0].content.as_ref(), "  ");
        assert_eq!(line.spans[1].content.as_ref(), " update ");
    }

    #[test]
    fn session_elapsed_label_compacts_by_density() {
        let elapsed = Duration::from_secs(65);
        assert_eq!(session_elapsed_label(elapsed, Density::Wide), "time 1m05s");
        assert_eq!(session_elapsed_label(elapsed, Density::Medium), "1m05s");
        assert_eq!(session_elapsed_label(elapsed, Density::Narrow), "1m");
    }

    #[test]
    fn composer_status_line_keeps_model_tokens_and_turn_state() {
        let mut app = test_app();
        app.session.input_tokens = 24_047;
        app.session.output_tokens = 74;
        app.session.total_tokens = 24_121;
        app.session.tool_call_count = 7;
        app.session.turn_input_tokens = 1_200;
        app.session.turn_output_tokens = 180;
        app.turn_status = TurnStatus::Done {
            elapsed: Duration::from_secs(86),
            tools: 3,
        };

        let line = spans_to_text(&composer_status_spans(&app, 120));

        assert!(line.contains("done 1m26s"));
        assert!(line.contains("model claude-sonnet-4-6-20260422"));
        assert!(line.contains("24.0k↑ 74↓ tok"));
        assert!(line.contains("turn 1.2k↑ 180↓"));
        assert!(line.contains("7 tools"));
    }

    #[test]
    fn composer_status_line_keeps_permission_mode_when_collapsed() {
        let mut app = test_app();
        app.session.permission_mode = crate::app::PermissionMode::AutoAccept;

        let line = spans_to_text(&composer_status_spans(&app, 50));

        assert!(line.contains("⚡A"));
        assert!(line.contains("idle"));
        assert!(line.contains("0↑0↓"));
    }

    #[test]
    fn composer_status_line_persists_done_after_turn_status_resets() {
        let mut app = test_app();
        app.session.turn_input_tokens = 10;
        app.session.turn_output_tokens = 20;
        app.turn_completion.last_turn_message =
            Some("Turn completed · 1.0s · 1 tool · 10↑ 20↓ tok".to_string());
        app.turn_status = TurnStatus::Idle;

        let line = spans_to_text(&composer_status_spans(&app, 90));

        assert!(line.contains("done"));
        assert!(line.contains("t 10↑ 20↓"));
    }

    #[test]
    fn composer_status_line_surfaces_pending_permission() {
        let mut app = test_app();
        app.pending_confirmation = Some(PendingConfirmation {
            id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: r#"{"command":"cargo test"}"#.to_string(),
        });

        let line = spans_to_text(&composer_status_spans(&app, 120));

        assert!(line.contains("permission"));
    }

    #[test]
    fn composer_status_line_surfaces_estimated_session_cost() {
        let mut app = test_app();
        app.session.input_tokens = 10_000;
        app.session.output_tokens = 1_000;
        app.session.total_tokens = 11_000;

        let line = spans_to_text(&composer_status_spans(&app, 120));

        assert!(line.contains("cost $0.0450"));
    }

    #[test]
    fn compact_model_name_truncates_long_runtime_ids() {
        assert_eq!(
            compact_model_name("anthropic/claude-sonnet-4-6-20260422", 18),
            "claude-sonnet-4..."
        );
    }
}
