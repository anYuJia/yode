use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;

const SEP: Color = Color::DarkGray;         // ANSI 8
const MUTED: Color = Color::Gray;            // ANSI 7
const LIGHT: Color = Color::White;           // ANSI 15 — bright

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

    let mut parts: Vec<Span> = Vec::new();

    // Prefix
    parts.push(Span::styled("  ", Style::default()));

    // Permission mode badge
    let mode = app.session.permission_mode.label();
    let (mode_icon, mode_color) = match app.session.permission_mode {
        crate::app::PermissionMode::Normal => ("●", Color::LightGreen),
        crate::app::PermissionMode::AutoAccept => ("⚡", Color::Yellow),
        crate::app::PermissionMode::Plan => ("📋", Color::LightBlue),
    };
    parts.push(Span::styled(
        format!("{} {} ", mode_icon, mode.to_lowercase()),
        Style::default().fg(mode_color),
    ));
    parts.push(Span::styled("· ", Style::default().fg(SEP)));

    // Token count (input↑ output↓)
    let input_prefix = if app.session.input_estimated { "~" } else { "" };
    parts.push(Span::styled(
        format!("{}{}↑ {}↓ tok ", input_prefix, app.session.input_tokens, app.session.output_tokens),
        Style::default().fg(LIGHT),
    ));
    parts.push(Span::styled("· ", Style::default().fg(SEP)));

    // Tool calls (with correct pluralization)
    if app.session.tool_call_count > 0 {
        let label = if app.session.tool_call_count == 1 { "call" } else { "calls" };
        parts.push(Span::styled(
            format!("{} {} ", app.session.tool_call_count, label),
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
        "ctx <1% ".to_string()
    } else {
        format!("ctx {:.0}% ", ctx_pct)
    };
    parts.push(Span::styled(ctx_str, Style::default().fg(ctx_color)));
    parts.push(Span::styled("· ", Style::default().fg(SEP)));

    // Queue
    if !app.pending_inputs.is_empty() {
        parts.push(Span::styled(
            format!("{} queued ", app.pending_inputs.len()),
            Style::default().fg(Color::LightMagenta),
        ));
        parts.push(Span::styled("· ", Style::default().fg(SEP)));
    }

    // Shortcuts hint
    parts.push(Span::styled("shift+tab mode", Style::default().fg(MUTED)));
    parts.push(Span::styled(" · ", Style::default().fg(SEP)));
    parts.push(Span::styled("/help", Style::default().fg(MUTED)));

    // Update notification (right-aligned)
    if let Some(ref version) = app.update_available {
        parts.push(Span::styled(" · ", Style::default().fg(SEP)));
        parts.push(Span::styled(
            format!("✨ Update available: {} (restart to apply)", version),
            Style::default().fg(Color::LightCyan),
        ));
    } else if app.update_downloading {
        parts.push(Span::styled(" · ", Style::default().fg(SEP)));
        parts.push(Span::styled(
            "⏳ Downloading update...",
            Style::default().fg(Color::Yellow),
        ));
    } else if let Some(ref version) = app.update_downloaded {
        parts.push(Span::styled(" · ", Style::default().fg(SEP)));
        parts.push(Span::styled(
            format!("✅ Update v{} ready (restart to apply)", version),
            Style::default().fg(Color::LightGreen),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(parts)), area);
}
