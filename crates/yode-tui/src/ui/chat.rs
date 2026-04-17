use super::chat_entries::{
    render_assistant, render_grouped_system_entries, render_grouped_tool_call,
    render_standalone_result, render_system_entry,
    render_tool_call,
    render_user,
};
use super::chat_layout::{manual_wrap, render_header};
use super::chat_markdown::render_markdown_impl;
use super::palette::{
    ERROR_COLOR, INFO_COLOR, LIGHT, MUTED, SUCCESS_COLOR, SURFACE_BG_ALT, TOOL_ACCENT, USER_COLOR,
    WARNING_COLOR,
};
use crate::app::{App, ChatRole};
use crate::tool_grouping::{detect_groupable_system_batch, detect_groupable_tool_batch};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

// ── Colors ──────────────────────────────────────────────────────────
// Use standard ANSI colors for grays — adapts to user's terminal color scheme.
// Color::White = ANSI 15 (bright white, usually #ffffff)
// Color::Gray = ANSI 7 (silver, usually #c0c0c0-#d0d0d0)
pub const GREEN: Color = SUCCESS_COLOR;
pub const RED: Color = ERROR_COLOR;
pub const YELLOW: Color = WARNING_COLOR;
pub const CYAN: Color = USER_COLOR;
pub const BLUE: Color = INFO_COLOR;
pub const DIM: Color = MUTED;
pub const WHITE: Color = LIGHT;
pub const CODE_BG: Color = Color::Indexed(234);
pub const INLINE_CODE_BG: Color = SURFACE_BG_ALT;
pub const ACCENT: Color = TOOL_ACCENT;

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
    let mut i = 0;
    let mut rendered_any_entry = false;
    while i < entries.len() {
        let entry = &entries[i];
        // Skip empty assistant
        if matches!(entry.role, ChatRole::Assistant) && entry.content.trim().is_empty() {
            i += 1;
            continue;
        }

        // Add separator between entries (blank line before each entry except the first)
        if rendered_any_entry {
            lines.push(Line::from(""));
        }

        match &entry.role {
            ChatRole::User => render_user(&mut lines, entry),
            ChatRole::Assistant => render_assistant(&mut lines, entry),
            ChatRole::ToolCall { id, name } => {
                if let Some(batch) = detect_groupable_tool_batch(entries, i) {
                    render_grouped_tool_call(&mut lines, entries, &batch);
                    rendered_any_entry = true;
                    i = batch.next_index;
                    continue;
                }
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
                    Span::styled(
                        "  ✕ ",
                        Style::default().fg(RED).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        entry.content.clone(),
                        Style::default().fg(RED).add_modifier(Modifier::BOLD),
                    ),
                ]));
            }
            ChatRole::System => {
                if let Some(batch) = detect_groupable_system_batch(entries, i) {
                    render_grouped_system_entries(&mut lines, entries, &batch);
                    rendered_any_entry = true;
                    i = batch.next_index;
                    continue;
                }
                render_system_entry(&mut lines, entry)
            }
            ChatRole::AskUser { .. } => {
                for (index, line) in entry.content.lines().enumerate() {
                    let prefix = if index == 0 { "  ? " } else { "    " };
                    lines.push(Line::from(vec![
                        Span::styled(
                            prefix.to_string(),
                            Style::default().fg(INFO_COLOR).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(line.to_string(), Style::default().fg(LIGHT)),
                    ]));
                }
            }
            ChatRole::SubAgentCall { .. }
            | ChatRole::SubAgentToolCall { .. }
            | ChatRole::SubAgentResult => {
                // These are rendered via scrollback printing, not the ratatui viewport
            }
        }
        rendered_any_entry = true;
        i += 1;
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
            Span::styled(format!("  {} ", spinner), Style::default().fg(INFO_COLOR)),
            Span::styled(
                "Working…",
                Style::default().fg(INFO_COLOR).add_modifier(Modifier::BOLD),
            ),
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
