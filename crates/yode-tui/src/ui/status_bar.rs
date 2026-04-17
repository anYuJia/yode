use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::badges::{
    budget_badge_label, permission_mode_badge, queue_badge_label, task_badge_label,
};
use super::palette::{LIGHT, MUTED, SEP};
use super::responsive::{density_from_width, status_section_mode, Density, StatusSectionMode};
use super::status_summary::{
    compaction_badge, context_badge, memory_badge, prompt_cache_badge, push_badge,
    runtime_family_badges, runtime_status_snapshot,
};
use crate::app::App;

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
    let section_mode = status_section_mode(area.width);
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
    if !matches!(section_mode, StatusSectionMode::Collapsed) {
        parts.push(Span::styled(mode_text, Style::default().fg(mode_color)));
        parts.push(Span::styled("· ", Style::default().fg(SEP)));
    }

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

#[cfg(test)]
mod tests {
    use super::Density;
    use crate::ui::badges::task_badge_label;
    use crate::ui::responsive::density_from_width;

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
}
