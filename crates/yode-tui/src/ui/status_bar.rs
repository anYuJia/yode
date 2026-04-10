use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;

const SEP: Color = Color::DarkGray; // ANSI 8
const MUTED: Color = Color::Gray; // ANSI 7
const LIGHT: Color = Color::White; // ANSI 15 — bright

#[derive(Clone, Copy, PartialEq, Eq)]
enum StatusBarDensity {
    Wide,
    Medium,
    Narrow,
}

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
    let density = status_bar_density(area.width);
    let running_tasks = running_task_count(app);
    let mut parts: Vec<Span> = Vec::new();

    // Prefix
    parts.push(Span::styled(
        if matches!(density, StatusBarDensity::Narrow) {
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
            StatusBarDensity::Wide => format!("{} {} ", mode_icon, mode.to_lowercase()),
            StatusBarDensity::Medium => format!("{} {} ", mode_icon, mode.to_lowercase()),
            StatusBarDensity::Narrow => format!("{}{} ", mode_icon, mode.chars().next().unwrap_or('m')),
        },
        Style::default().fg(mode_color),
    ));
    parts.push(Span::styled("· ", Style::default().fg(SEP)));

    // Token count (input↑ output↓)
    let input_prefix = if app.session.input_estimated { "~" } else { "" };
    parts.push(Span::styled(
        match density {
            StatusBarDensity::Wide => format!(
                "{}{}↑ {}↓ tok ",
                input_prefix, app.session.input_tokens, app.session.output_tokens
            ),
            StatusBarDensity::Medium => format!(
                "{}{}↑ {}↓ ",
                input_prefix, app.session.input_tokens, app.session.output_tokens
            ),
            StatusBarDensity::Narrow => format!(
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
                StatusBarDensity::Wide => format!("{} {} ", app.session.tool_call_count, label),
                StatusBarDensity::Medium | StatusBarDensity::Narrow => {
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
        if matches!(density, StatusBarDensity::Wide) {
            "ctx <1% ".to_string()
        } else {
            "c<1 ".to_string()
        }
    } else {
        match density {
            StatusBarDensity::Wide => format!("ctx {:.0}% ", ctx_pct),
            StatusBarDensity::Medium | StatusBarDensity::Narrow => format!("c{:.0}% ", ctx_pct),
        }
    };
    parts.push(Span::styled(ctx_str, Style::default().fg(ctx_color)));
    parts.push(Span::styled("· ", Style::default().fg(SEP)));

    // Queue
    if !app.pending_inputs.is_empty() {
        parts.push(Span::styled(
            match density {
                StatusBarDensity::Wide => format!("{} queued ", app.pending_inputs.len()),
                StatusBarDensity::Medium | StatusBarDensity::Narrow => {
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
        StatusBarDensity::Wide => {
            parts.push(Span::styled("shift+tab mode", Style::default().fg(MUTED)));
            parts.push(Span::styled(" · ", Style::default().fg(SEP)));
            parts.push(Span::styled("/help", Style::default().fg(MUTED)));
        }
        StatusBarDensity::Medium => {
            parts.push(Span::styled("tab mode", Style::default().fg(MUTED)));
            parts.push(Span::styled(" · ", Style::default().fg(SEP)));
            parts.push(Span::styled("/help", Style::default().fg(MUTED)));
        }
        StatusBarDensity::Narrow => {
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

fn status_bar_density(width: u16) -> StatusBarDensity {
    if width < 68 {
        StatusBarDensity::Narrow
    } else if width < 96 {
        StatusBarDensity::Medium
    } else {
        StatusBarDensity::Wide
    }
}

fn task_badge_label(count: usize, density: StatusBarDensity) -> String {
    match density {
        StatusBarDensity::Wide => format!("{} tasks ", count),
        StatusBarDensity::Medium => format!("t{} ", count),
        StatusBarDensity::Narrow => format!("{}t ", count),
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

fn tool_budget_badge(app: &App, density: StatusBarDensity) -> Option<String> {
    if app.turn_tool_count >= 25 {
        return Some(match density {
            StatusBarDensity::Wide => "budget warning ".to_string(),
            StatusBarDensity::Medium => "budget! ".to_string(),
            StatusBarDensity::Narrow => "!b ".to_string(),
        });
    }
    if app.turn_tool_count >= 15 {
        return Some(match density {
            StatusBarDensity::Wide => "budget notice ".to_string(),
            StatusBarDensity::Medium => "budget ".to_string(),
            StatusBarDensity::Narrow => "b ".to_string(),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{status_bar_density, task_badge_label, StatusBarDensity};

    #[test]
    fn status_bar_density_compacts_on_narrow_widths() {
        assert!(matches!(status_bar_density(120), StatusBarDensity::Wide));
        assert!(matches!(status_bar_density(80), StatusBarDensity::Medium));
        assert!(matches!(status_bar_density(50), StatusBarDensity::Narrow));
    }

    #[test]
    fn task_badge_label_compacts_for_small_widths() {
        assert_eq!(task_badge_label(3, StatusBarDensity::Wide), "3 tasks ");
        assert_eq!(task_badge_label(3, StatusBarDensity::Medium), "t3 ");
        assert_eq!(task_badge_label(3, StatusBarDensity::Narrow), "3t ");
    }
}
