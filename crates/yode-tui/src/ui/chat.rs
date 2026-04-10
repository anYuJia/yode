use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, ChatRole};
use super::chat_entries::{
    render_assistant, render_standalone_result, render_tool_call, render_user,
};
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

// ── Scroll calculation ──────────────────────────────────────────────

// ── Manual line wrapping ────────────────────────────────────────────

/// Wrap lines at `width` using unicode display widths.
/// Returns a new Vec where each Line fits in one visual row.
/// This gives us lines.len() == exact visual row count, no estimation needed.
pub fn manual_wrap(lines: Vec<Line<'static>>, width: u16) -> Vec<Line<'static>> {
    let w = width.max(1) as usize;
    let mut result = Vec::with_capacity(lines.len());

    for line in lines {
        let total_w: usize = line
            .spans
            .iter()
            .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
            .sum();

        if total_w <= w {
            // Fits in one row — keep as is
            result.push(line);
        } else {
            // Need to wrap — split spans across multiple rows
            let mut current_spans: Vec<Span<'static>> = Vec::new();
            let mut current_w: usize = 0;

            for span in line.spans {
                let span_w = UnicodeWidthStr::width(span.content.as_ref());
                if current_w + span_w <= w {
                    current_w += span_w;
                    current_spans.push(span);
                } else {
                    // Need to split this span character by character
                    let mut buf = String::new();
                    let style = span.style;
                    for ch in span.content.chars() {
                        let ch_w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                        if current_w + ch_w > w && !buf.is_empty() {
                            // Flush current line
                            current_spans.push(Span::styled(buf.clone(), style));
                            result.push(Line::from(current_spans));
                            current_spans = Vec::new();
                            current_w = 0;
                            buf.clear();
                        }
                        buf.push(ch);
                        current_w += ch_w;
                    }
                    if !buf.is_empty() {
                        current_spans.push(Span::styled(buf, style));
                    }
                }
            }
            if !current_spans.is_empty() {
                result.push(Line::from(current_spans));
            }
        }
    }

    result
}

// ── Header (info left, YODE logo right) ───────────────────────────
pub fn render_header(app: &App, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let title_style = Style::default().fg(ACCENT).add_modifier(Modifier::BOLD);
    let ver_style = Style::default().fg(DIM);
    let model_style = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let path_style = Style::default().fg(GREEN);
    let dim = Style::default().fg(DIM);
    let hint_style = Style::default().fg(DIM);
    let session_short = if app.session.session_id.len() >= 8 {
        app.session.session_id[..8].to_string()
    } else {
        app.session.session_id.clone()
    };
    let model = app.session.model.clone();
    let workdir = app.session.working_dir.clone();

    // YODE logo (34 display cols) — uses Indexed colors for Terminal.app compat
    let logo = [
        "██╗   ██╗ ██████╗ ██████╗ ███████╗",
        "╚██╗ ██╔╝██╔═══██╗██╔══██╗██╔════╝",
        " ╚████╔╝ ██║   ██║██║  ██║█████╗  ",
        "  ╚██╔╝  ██║   ██║██║  ██║██╔══╝  ",
        "   ██║   ╚██████╔╝██████╔╝███████╗",
        "   ╚═╝    ╚═════╝ ╚═════╝ ╚══════╝",
    ];
    let logo_w = 34usize;
    // Gradient colors for border + logo (cyan/blue/green range, ANSI 256)
    let gradient: [Color; 8] = [
        Color::Indexed(37),  // top border
        Color::Indexed(37),  // row 0 (logo[0])
        Color::Indexed(44),  // row 1 (logo[1])
        Color::Indexed(45),  // row 2 (logo[2])
        Color::Indexed(81),  // row 3 (logo[3])
        Color::Indexed(115), // row 4 (logo[4])
        Color::Indexed(120), // row 5 (logo[5])
        Color::Indexed(120), // bottom border
    ];

    let inner_w = width.saturating_sub(4);
    let show_logo = inner_w > logo_w + 25;

    // Helper: build a row with left content + optional right-aligned logo
    // `row_idx` is the gradient index for the left border
    let make_row = |left_spans: Vec<Span<'static>>,
                    logo_idx: Option<usize>,
                    row_idx: usize|
     -> Line<'static> {
        let left_w: usize = left_spans
            .iter()
            .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
            .sum();

        let border_color = Style::default().fg(gradient[row_idx]);
        let mut spans = vec![Span::styled("│ ", border_color)];
        spans.extend(left_spans);

        if show_logo {
            if let Some(idx) = logo_idx {
                let gap = inner_w.saturating_sub(left_w + logo_w);
                spans.push(Span::raw(" ".repeat(gap)));
                spans.push(Span::styled(
                    logo[idx].to_string(),
                    Style::default()
                        .fg(gradient[row_idx])
                        .add_modifier(Modifier::BOLD),
                ));
            }
        }

        Line::from(spans)
    };

    // ── Title line: ╭ Yode vX.Y.Z ────────╮
    let title_text = " Yode ";
    let ver_text = concat!("v", env!("CARGO_PKG_VERSION"), " ");
    let rule_len = width.saturating_sub(title_text.len() + ver_text.len() + 2);
    let top_color = Style::default().fg(gradient[0]);
    lines.push(Line::from(vec![
        Span::styled("╭", top_color),
        Span::styled(title_text, title_style),
        Span::styled(ver_text, ver_style),
        Span::styled("─".repeat(rule_len), top_color),
        Span::styled("╮", top_color),
    ]));

    // Row 0: empty + logo[0]
    lines.push(make_row(vec![], Some(0), 1));

    // Row 1: model + logo[1]
    lines.push(make_row(
        vec![
            Span::styled(" ", Style::default()),
            Span::styled(model, model_style),
        ],
        Some(1),
        2,
    ));

    // Row 2: workdir + logo[2]
    lines.push(make_row(
        vec![
            Span::styled(" ", Style::default()),
            Span::styled(workdir, path_style),
        ],
        Some(2),
        3,
    ));

    // Row 3: session + logo[3]
    lines.push(make_row(
        vec![
            Span::styled(" ", Style::default()),
            Span::styled("agentic terminal · ", Style::default().fg(ACCENT)),
            Span::styled(format!("session {}", session_short), dim),
        ],
        Some(3),
        4,
    ));

    // Row 4: empty + logo[4]
    lines.push(make_row(vec![], Some(4), 5));

    // Row 5: tips + logo[5]
    lines.push(make_row(
        vec![
            Span::styled(" ", Style::default()),
            Span::styled("? ", Style::default().fg(ACCENT)),
            Span::styled("/help", hint_style),
            Span::styled(" · ", Style::default().fg(Color::DarkGray)),
            Span::styled("/keys", hint_style),
            Span::styled(" · ", Style::default().fg(Color::DarkGray)),
            Span::styled("Shift+Tab mode", hint_style),
            Span::styled(" · ", Style::default().fg(Color::DarkGray)),
            Span::styled("Ctrl+C×2 quit", hint_style),
        ],
        Some(5),
        6,
    ));

    // ── Bottom rule: ╰─────────────────────╯
    let bot_color = Style::default().fg(gradient[7]);
    lines.push(Line::from(vec![
        Span::styled("╰", bot_color),
        Span::styled("─".repeat(width.saturating_sub(2)), bot_color),
        Span::styled("╯", bot_color),
    ]));

    lines
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
