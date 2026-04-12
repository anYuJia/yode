use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use super::palette::{LIGHT, MUTED, SEP};
use super::responsive::{density_from_width, Density};

/// Top separator line: ────────────────────────────
pub fn render_separator(frame: &mut Frame, area: Rect) {
    let line = Line::from(Span::styled(
        "─".repeat(area.width as usize),
        Style::default().fg(SEP),
    ));
    frame.render_widget(Paragraph::new(line), area);
}

/// Bottom info line with session details:
///   ⚡ mode · 120↑ 437↓ tok · 1 call · ctx 2% · /help
pub fn render_info_line(frame: &mut Frame, area: Rect, app: &App) {
    let density = density_from_width(area.width, 68, 96);
    let running_tasks = running_task_count(app);
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
    let mode = app.session.permission_mode.label();
    let (mode_icon, mode_color) = match app.session.permission_mode {
        crate::app::PermissionMode::Normal => ("●", Color::LightGreen),
        crate::app::PermissionMode::AutoAccept => ("⚡", Color::Yellow),
        crate::app::PermissionMode::Plan => ("📋", Color::LightBlue),
    };
    parts.push(Span::styled(
        match density {
            Density::Wide => format!("{} {} ", mode_icon, mode.to_lowercase()),
            Density::Medium => format!("{} {} ", mode_icon, mode.to_lowercase()),
            Density::Narrow => {
                format!("{}{} ", mode_icon, mode.chars().next().unwrap_or('m'))
            }
        },
        Style::default().fg(mode_color),
    ));
    parts.push(Span::styled("· ", Style::default().fg(SEP)));

    // Token count (input↑ output↓)
    let input_prefix = if app.session.input_estimated { "~" } else { "" };
    parts.push(Span::styled(
        match density {
            Density::Wide => format!(
                "{}{}↑ {}↓ tok ",
                input_prefix, app.session.input_tokens, app.session.output_tokens
            ),
            Density::Medium => format!(
                "{}{}↑ {}↓ ",
                input_prefix, app.session.input_tokens, app.session.output_tokens
            ),
            Density::Narrow => format!(
                "{}{}↑{}↓ ",
                input_prefix, app.session.input_tokens, app.session.output_tokens
            ),
        },
        Style::default().fg(LIGHT),
    ));
    parts.push(Span::styled("· ", Style::default().fg(SEP)));

    // Tool calls (with correct pluralization)
    if app.session.tool_call_count > 0 {
        let label = if app.session.tool_call_count == 1 {
            "call"
        } else {
            "calls"
        };
        parts.push(Span::styled(
            match density {
                Density::Wide => format!("{} {} ", app.session.tool_call_count, label),
                Density::Medium | Density::Narrow => {
                    format!("{}c ", app.session.tool_call_count)
                }
            },
            Style::default().fg(LIGHT),
        ));
        parts.push(Span::styled("· ", Style::default().fg(SEP)));
    }

    // Context window estimate
    let total_chars: usize = app.chat_entries.iter().map(|e| e.content.len()).sum();
    let ctx_tokens = total_chars / 4;
    let ctx_pct = if ctx_tokens > 0 {
        (ctx_tokens as f64 / 128000.0 * 100.0).min(100.0)
    } else {
        0.0
    };
    let ctx_color = if ctx_pct > 80.0 {
        Color::LightRed // red when high
    } else if ctx_pct > 50.0 {
        Color::Yellow // yellow
    } else {
        LIGHT
    };
    let ctx_str = if ctx_pct > 0.0 && ctx_pct < 1.0 {
        if matches!(density, Density::Wide) {
            "ctx <1% ".to_string()
        } else {
            "c<1 ".to_string()
        }
    } else {
        match density {
            Density::Wide => format!("ctx {:.0}% ", ctx_pct),
            Density::Medium | Density::Narrow => format!("c{:.0}% ", ctx_pct),
        }
    };
    parts.push(Span::styled(ctx_str, Style::default().fg(ctx_color)));
    parts.push(Span::styled("· ", Style::default().fg(SEP)));

    // Queue
    if !app.pending_inputs.is_empty() {
        parts.push(Span::styled(
            match density {
                Density::Wide => format!("{} queued ", app.pending_inputs.len()),
                Density::Medium | Density::Narrow => {
                    format!("q{} ", app.pending_inputs.len())
                }
            },
            Style::default().fg(Color::LightMagenta),
        ));
        parts.push(Span::styled("· ", Style::default().fg(SEP)));
    }

    if running_tasks > 0 {
        parts.push(Span::styled(
            task_badge_label(running_tasks, density),
            Style::default().fg(Color::LightBlue),
        ));
        parts.push(Span::styled("· ", Style::default().fg(SEP)));
    }

    if let Some(budget_badge) = tool_budget_badge(app, density) {
        parts.push(Span::styled(
            budget_badge,
            Style::default().fg(Color::LightYellow),
        ));
        parts.push(Span::styled("· ", Style::default().fg(SEP)));
    }

    // Shortcuts hint
    match density {
        Density::Wide => {
            parts.push(Span::styled("shift+tab mode", Style::default().fg(MUTED)));
            parts.push(Span::styled(" · ", Style::default().fg(SEP)));
            parts.push(Span::styled("/help", Style::default().fg(MUTED)));
        }
        Density::Medium => {
            parts.push(Span::styled("tab mode", Style::default().fg(MUTED)));
            parts.push(Span::styled(" · ", Style::default().fg(SEP)));
            parts.push(Span::styled("/help", Style::default().fg(MUTED)));
        }
        Density::Narrow => {
            parts.push(Span::styled("/h", Style::default().fg(MUTED)));
        }
    }

    frame.render_widget(Paragraph::new(Line::from(parts)), area);
}

/// Bottom blank line: renders a row of space characters
/// This keeps the line visually present (not collapsed) while appearing empty.
pub fn render_blank_line(frame: &mut Frame, area: Rect, app: &App) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let mut update_text = String::new();
    let mut update_style = Style::default();

    if let Some(ref version) = app.update_available {
        update_text = format!(" ✨ Update available: {} (restart to apply) ", version);
        update_style = Style::default().fg(Color::LightCyan);
    } else if app.update_downloading {
        update_text = " ⏳ Downloading update... ".to_string();
        update_style = Style::default().fg(Color::Yellow);
    } else if let Some(ref version) = app.update_downloaded {
        update_text = format!(" ✅ Update v{} ready (restart to apply) ", version);
        update_style = Style::default().fg(Color::LightGreen);
    }

    let update_len = update_text.chars().count();
    let left_dots_len = area.width.saturating_sub(update_len as u16) as usize;

    let mut parts = vec![Span::styled(
        "·".repeat(left_dots_len),
        Style::default().fg(SEP),
    )];

    if update_len > 0 {
        parts.push(Span::styled(update_text, update_style));
    }

    frame.render_widget(Paragraph::new(Line::from(parts)), area);
}

fn task_badge_label(count: usize, density: Density) -> String {
    match density {
        Density::Wide => format!("{} tasks ", count),
        Density::Medium => format!("t{} ", count),
        Density::Narrow => format!("{}t ", count),
    }
}

fn running_task_count(app: &App) -> usize {
    app.engine
        .as_ref()
        .and_then(|engine| engine.try_lock().ok())
        .map(|engine| {
            engine
                .runtime_tasks_snapshot()
                .into_iter()
                .filter(|task| matches!(task.status, yode_tools::RuntimeTaskStatus::Running))
                .count()
        })
        .unwrap_or(0)
}

fn tool_budget_badge(app: &App, density: Density) -> Option<String> {
    if app.turn_tool_count >= 25 {
        return Some(match density {
            Density::Wide => "budget warning ".to_string(),
            Density::Medium => "budget! ".to_string(),
            Density::Narrow => "!b ".to_string(),
        });
    }
    if app.turn_tool_count >= 15 {
        return Some(match density {
            Density::Wide => "budget notice ".to_string(),
            Density::Medium => "budget ".to_string(),
            Density::Narrow => "b ".to_string(),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{task_badge_label, Density};
    use crate::ui::responsive::density_from_width;

    #[test]
    fn status_bar_density_compacts_on_narrow_widths() {
        assert!(matches!(density_from_width(120, 68, 96), Density::Wide));
        assert!(matches!(density_from_width(80, 68, 96), Density::Medium));
        assert!(matches!(density_from_width(50, 68, 96), Density::Narrow));
    }

    #[test]
    fn task_badge_label_compacts_for_small_widths() {
        assert_eq!(task_badge_label(3, Density::Wide), "3 tasks ");
        assert_eq!(task_badge_label(3, Density::Medium), "t3 ");
        assert_eq!(task_badge_label(3, Density::Narrow), "3t ");
    }
}
