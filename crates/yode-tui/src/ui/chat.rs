use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use crate::app::{App, ChatRole};
use super::chat_entries::{
    render_assistant, render_standalone_result, render_tool_call, render_user,
};
use super::chat_layout::{manual_wrap, render_header};
use super::chat_markdown::render_markdown_impl;

// ── Colors ──────────────────────────────────────────────────────────
// Use standard ANSI colors for grays — adapts to user's terminal color scheme.
// Color::White = ANSI 15 (bright white, usually #ffffff)
// Color::Gray = ANSI 7 (silver, usually #c0c0c0-#d0d0d0)
pub const GREEN: Color = Color::LightGreen;
pub const RED: Color = Color::LightRed;
pub const YELLOW: Color = Color::LightYellow;
pub const CYAN: Color = Color::Indexed(51); // RGB #00FFFF - pure cyan (most visible)
pub const BLUE: Color = Color::LightBlue;
pub const DIM: Color = Color::Gray; // ANSI 7 — adapts to terminal theme
pub const WHITE: Color = Color::Indexed(231); // RGB #FFFFFF - pure white
pub const CODE_BG: Color = Color::Indexed(234); // #1c1c1c
pub const INLINE_CODE_BG: Color = Color::Indexed(236); // #303030
pub const ACCENT: Color = Color::LightCyan; // ANSI 14 — crisp terminal cyan

// ── Main Render ─────────────────────────────────────────────────────
pub fn render_chat(frame: &mut Frame, area: Rect, app: &App) -> u16 {
    let mut lines: Vec<Line> = Vec::new();

    // Header
    lines.extend(render_header(app, area.width as usize));
    lines.push(Line::from(""));

    if app.chat_entries.is_empty() {
        lines.push(Line::from(Span::styled(
            "  Type your request to get started.",
            Style::default().fg(DIM),
        )));
    }

    let entries = &app.chat_entries;
    for (i, entry) in entries.iter().enumerate() {
        // Skip empty assistant
        if matches!(entry.role, ChatRole::Assistant) && entry.content.trim().is_empty() {
            continue;
        }

        // Add separator between entries (blank line before each entry except the first)
        if i > 0 {
            lines.push(Line::from(""));
        }

        match &entry.role {
            ChatRole::User => render_user(&mut lines, entry),
            ChatRole::Assistant => render_assistant(&mut lines, entry),
            ChatRole::ToolCall { id, name } => {
                // Find matching ToolResult (next entry with same ID)
                let result_entry = entries[i + 1..]
                    .iter()
                    .find(|e| matches!(&e.role, ChatRole::ToolResult { id: eid, .. } if eid == id));
                render_tool_call(
                    &mut lines,
                    name,
                    &entry.content,
                    result_entry,
                    entry.progress.as_ref(),
                    entry.timestamp,
                );
            }
            ChatRole::ToolResult { id, .. } => {
                // Already rendered as part of ToolCall above — skip standalone
                // But if there was no preceding ToolCall, render it
                let has_preceding_call = i > 0
                    && entries[..i].iter().rev().any(
                        |e| matches!(&e.role, ChatRole::ToolCall { id: tid, .. } if tid == id),
                    );
                if !has_preceding_call {
                    render_standalone_result(&mut lines, entry);
                }
            }
            ChatRole::Error => {
                lines.push(Line::from(vec![
                    Span::styled("! ", Style::default().fg(RED).add_modifier(Modifier::BOLD)),
                    Span::styled(entry.content.clone(), Style::default().fg(RED)),
                ]));
            }
            ChatRole::System | ChatRole::AskUser { .. } => {
                for line in entry.content.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", line),
                        Style::default().fg(DIM),
                    )));
                }
            }
            ChatRole::SubAgentCall { .. }
            | ChatRole::SubAgentToolCall { .. }
            | ChatRole::SubAgentResult => {
                // These are rendered via scrollback printing, not the ratatui viewport
            }
        }
    }

    // Thinking indicator at the bottom of chat
    if app.is_thinking {
        // Add spacing before thinking if there are previous entries
        if !entries.is_empty() && !lines.is_empty() {
            lines.push(Line::from(""));
        }

        let spinner = app.spinner_char();
        let elapsed_str = app.thinking_elapsed_str();
        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", spinner), Style::default().fg(YELLOW)),
            Span::styled("Working…", Style::default().fg(YELLOW)),
            Span::styled(format!(" ({})", elapsed_str), Style::default().fg(DIM)),
        ]));
        lines.push(Line::from(""));
    }

    // ── Manual wrapping ────────────────────────────────────────
    let wrapped = manual_wrap(lines, area.width);
    let content_height = wrapped.len() as u16;

    let paragraph = Paragraph::new(wrapped);
    frame.render_widget(paragraph, area);

    content_height
}

// ── Markdown Renderer ───────────────────────────────────────────────
pub fn render_markdown(text: &str) -> Vec<Line<'static>> {
    render_markdown_impl(text, None)
}

/// Render markdown with white foreground color (for assistant messages).
pub fn render_markdown_white(text: &str) -> Vec<Line<'static>> {
    render_markdown_impl(text, Some(WHITE))
}

// ── Helpers ─────────────────────────────────────────────────────────
