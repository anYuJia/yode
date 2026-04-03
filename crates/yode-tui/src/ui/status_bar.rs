use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;

const SEP: Color = Color::Indexed(245);    // #8a8a8a
const MUTED: Color = Color::Indexed(249);   // #b2b2b2
const LIGHT: Color = Color::Indexed(252);   // #d0d0d0

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
        crate::app::PermissionMode::Normal => ("●", Color::Rgb(80, 200, 120)),
        crate::app::PermissionMode::AutoAccept => ("⚡", Color::Rgb(240, 180, 50)),
        crate::app::PermissionMode::Plan => ("📋", Color::Rgb(100, 180, 255)),
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
        Color::Rgb(240, 80, 80) // red when high
    } else if ctx_pct > 50.0 {
        Color::Rgb(240, 180, 50) // yellow
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
            Style::default().fg(Color::Rgb(200, 140, 255)),
        ));
        parts.push(Span::styled("· ", Style::default().fg(SEP)));
    }

    // Shortcuts hint
    parts.push(Span::styled("shift+tab mode", Style::default().fg(MUTED)));
    parts.push(Span::styled(" · ", Style::default().fg(SEP)));
    parts.push(Span::styled("/help", Style::default().fg(MUTED)));

    frame.render_widget(Paragraph::new(Line::from(parts)), area);
}
