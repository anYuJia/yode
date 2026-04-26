use super::chat_entries::{
    render_assistant, render_grouped_subagent_batch, render_grouped_system_entries,
    render_grouped_tool_call, render_standalone_result, render_subagent_call, render_system_entry,
    render_tool_call, render_user,
};
use super::chat_layout::{manual_wrap, render_header};
use super::chat_markdown::render_markdown_impl;
use super::palette::{
    ERROR_COLOR, INFO_COLOR, LIGHT, MUTED, SUCCESS_COLOR, SURFACE_BG_ALT, TOOL_ACCENT, USER_COLOR,
    WARNING_COLOR,
};
use super::turn_status::active_working_label;
use crate::app::{App, ChatRole};
use crate::tool_grouping::{
    detect_groupable_subagent_batch, detect_groupable_system_batch, detect_groupable_tool_batch,
    should_hide_tool_from_transcript,
};
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
            ChatRole::Assistant => render_assistant(
                &mut lines,
                entry,
                area.width.saturating_sub(2) as usize,
                app.terminal_caps.supports_hyperlinks(),
            ),
            ChatRole::ToolCall { id, name } => {
                if should_hide_tool_from_transcript(name) {
                    i += 1;
                    continue;
                }
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
                if let ChatRole::ToolResult { name, .. } = &entry.role {
                    if should_hide_tool_from_transcript(name) {
                        i += 1;
                        continue;
                    }
                }
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
            ChatRole::SubAgentCall { description } => {
                if let Some(batch) = detect_groupable_subagent_batch(entries, i) {
                    render_grouped_subagent_batch(&mut lines, entries, &batch);
                    rendered_any_entry = true;
                    i = batch.next_index;
                    continue;
                }
                render_subagent_call(&mut lines, description, entries, i);
            }
            ChatRole::SubAgentToolCall { .. } | ChatRole::SubAgentResult => {
                i += 1;
                continue;
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
        let working_label = active_working_label(app, "Working");
        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", spinner), Style::default().fg(INFO_COLOR)),
            Span::styled(
                working_label,
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

pub fn render_markdown_white_with_options(
    text: &str,
    max_width: Option<usize>,
    enable_hyperlinks: bool,
) -> Vec<Line<'static>> {
    super::chat_markdown::render_markdown_with_options(
        text,
        Some(WHITE),
        super::chat_markdown::MarkdownRenderOptions {
            max_width,
            enable_hyperlinks,
        },
    )
}

pub(crate) fn render_markdown_ansi_white_with_options(
    text: &str,
    max_width: Option<usize>,
    enable_hyperlinks: bool,
) -> Vec<String> {
    super::chat_markdown::render_markdown_ansi_with_options(
        text,
        Some(WHITE),
        super::chat_markdown::MarkdownRenderOptions {
            max_width,
            enable_hyperlinks,
        },
    )
}

pub(crate) fn render_markdown_ansi_dim_with_options(
    text: &str,
    max_width: Option<usize>,
    enable_hyperlinks: bool,
) -> Vec<String> {
    super::chat_markdown::render_markdown_ansi_with_options(
        text,
        Some(DIM),
        super::chat_markdown::MarkdownRenderOptions {
            max_width,
            enable_hyperlinks,
        },
    )
}

pub(crate) fn streaming_markdown_advance_stable_boundary(
    text: &str,
    current_stable_len: usize,
) -> usize {
    super::chat_markdown::streaming_markdown_advance_stable_boundary(text, current_stable_len)
}

// ── Helpers ─────────────────────────────────────────────────────────
