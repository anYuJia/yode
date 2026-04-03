use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, ChatEntry, ChatRole};

// ── Colors ──────────────────────────────────────────────────────────
// Use AnsiValue for grays — universally supported (Terminal.app ignores RGB escapes)
// Grayscale ramp: 232(#080808) … 249(#b2b2b2) … 252(#d0d0d0) … 255(#eeeeee)
pub const GREEN: Color = Color::Rgb(78, 186, 101);
pub const RED: Color = Color::Rgb(255, 107, 128);
pub const YELLOW: Color = Color::Rgb(255, 193, 7);
pub const CYAN: Color = Color::Rgb(100, 200, 220);
pub const BLUE: Color = Color::Rgb(147, 165, 255);
pub const DIM: Color = Color::Indexed(250);        // #bcbcbc
pub const WHITE: Color = Color::Indexed(255);       // #eeeeee
pub const CODE_BG: Color = Color::Indexed(234);     // #1c1c1c
pub const INLINE_CODE_BG: Color = Color::Indexed(236); // #303030
pub const ACCENT: Color = Color::Rgb(175, 135, 255); // purple for ⏺

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

        // Add separator between entries (only a single blank line, not after last)
        if i > 0 && !lines.is_empty() {
            lines.push(Line::from(""));
        }

        match &entry.role {
            ChatRole::User => render_user(&mut lines, entry),
            ChatRole::Assistant => render_assistant(&mut lines, entry),
            ChatRole::ToolCall { name } => {
                // Find matching ToolResult (next entry with same tool name)
                let result_entry = entries[i + 1..].iter().find(|e| {
                    matches!(&e.role, ChatRole::ToolResult { name: n, .. } if n == name)
                });
                render_tool_call(&mut lines, name, &entry.content, result_entry);
            }
            ChatRole::ToolResult { .. } => {
                // Already rendered as part of ToolCall above — skip standalone
                // But if there was no preceding ToolCall, render it
                let has_preceding_call = i > 0 && entries[..i].iter().rev().any(|e| {
                    matches!(&e.role, ChatRole::ToolCall { name: n } if {
                        if let ChatRole::ToolResult { name: rn, .. } = &entry.role { n == rn } else { false }
                    })
                });
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
            ChatRole::Retrying => {
                for line in entry.content.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", line),
                        Style::default().fg(YELLOW),
                    )));
                }
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
        let spinner = app.spinner_char();
        let elapsed_str = app.thinking_elapsed_str();
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {} ", spinner),
                Style::default().fg(YELLOW),
            ),
            Span::styled(
                "Working…",
                Style::default().fg(YELLOW),
            ),
            Span::styled(
                format!(" ({})", elapsed_str),
                Style::default().fg(DIM),
            ),
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

// ── User Message ────────────────────────────────────────────────────
// Claude Code style: just bold white text, no heavy decoration
pub fn render_user(lines: &mut Vec<Line<'static>>, entry: &ChatEntry) {
    for (i, line) in entry.content.lines().enumerate() {
        if i == 0 {
            lines.push(Line::from(vec![
                Span::styled("> ", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
                Span::styled(line.to_string(), Style::default().fg(WHITE).add_modifier(Modifier::BOLD)),
            ]));
        } else {
            lines.push(Line::from(Span::styled(
                format!("  {}", line),
                Style::default().fg(WHITE),
            )));
        }
    }
}

// ── Assistant Message ───────────────────────────────────────────────
// Claude Code style: ⏺ prefix on first line, indented continuation
pub fn render_assistant(lines: &mut Vec<Line<'static>>, entry: &ChatEntry) {
    let md = render_markdown(&entry.content);
    for (i, line) in md.into_iter().enumerate() {
        let mut spans = Vec::new();
        if i == 0 {
            spans.push(Span::styled("⏺ ", Style::default().fg(ACCENT)));
        } else {
            spans.push(Span::raw("  "));
        }
        spans.extend(line.spans);
        lines.push(Line::from(spans));
    }
}

// ── Tool Call + Result (combined) ───────────────────────────────────
// Claude Code style: ⏺ ToolName(description) then ⎿ result
pub fn render_tool_call(
    lines: &mut Vec<Line<'static>>,
    name: &str,
    args_json: &str,
    result: Option<&ChatEntry>,
) {
    let args: serde_json::Value = serde_json::from_str(args_json).unwrap_or_default();
    let is_error = result.map_or(false, |r| matches!(r.role, ChatRole::ToolResult { is_error, .. } if is_error));
    let result_content = result.map(|r| r.content.as_str()).unwrap_or("");

    // ⏺ ToolName(summary)
    let summary = tool_summary(name, &args);
    let tool_display = capitalize_tool(name);

    lines.push(Line::from(vec![
        Span::styled("⏺ ", Style::default().fg(ACCENT)),
        Span::styled(
            format!("{}(", tool_display),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            truncate_str(&summary, 80),
            Style::default().fg(DIM),
        ),
        Span::styled(")", Style::default().fg(WHITE).add_modifier(Modifier::BOLD)),
    ]));

    // Tool-specific content rendering
    match name {
        "bash" => render_bash_content(lines, &args, result_content, is_error),
        "write_file" => render_write_content(lines, &args, is_error),
        "edit_file" => render_edit_content(lines, &args, is_error),
        _ => {}
    }

    // ⎿ result
    if !result_content.is_empty() {
        let output_lines: Vec<&str> = result_content.lines().collect();
        let max_show = 8;
        let show = output_lines.len().min(max_show);

        let result_color = if is_error { RED } else { DIM };

        for (i, line) in output_lines[..show].iter().enumerate() {
            let prefix = if i == 0 { "  ⎿  " } else { "     " };
            lines.push(Line::from(Span::styled(
                format!("{}{}", prefix, line),
                Style::default().fg(result_color),
            )));
        }
        if output_lines.len() > max_show {
            lines.push(Line::from(Span::styled(
                format!("     … {} more lines", output_lines.len() - max_show),
                Style::default().fg(DIM),
            )));
        }
    }
}

/// Get a one-line summary for a tool call.
fn tool_summary(name: &str, args: &serde_json::Value) -> String {
    match name {
        "bash" => args["command"].as_str().unwrap_or("???").to_string(),
        "write_file" | "read_file" => args["file_path"].as_str().unwrap_or("???").to_string(),
        "edit_file" => {
            let path = args["file_path"].as_str().unwrap_or("???");
            format!("{}", shorten_path(path))
        }
        "glob" => args["pattern"].as_str().unwrap_or("???").to_string(),
        "grep" => args["pattern"].as_str().unwrap_or("???").to_string(),
        _ => {
            // Extract first string-valued argument
            if let Some(obj) = args.as_object() {
                for key in &["command", "path", "file_path", "query", "pattern", "url"] {
                    if let Some(val) = obj.get(*key) {
                        if let Some(s) = val.as_str() {
                            return s.to_string();
                        }
                    }
                }
            }
            String::new()
        }
    }
}

/// Capitalize tool name for display: "bash" → "Bash"
fn capitalize_tool(name: &str) -> String {
    let mut c = name.chars();
    match c.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().to_string() + c.as_str(),
    }
}

// ── bash content ────────────────────────────────────────────────────
fn render_bash_content(
    lines: &mut Vec<Line<'static>>,
    args: &serde_json::Value,
    _result: &str,
    _is_error: bool,
) {
    let cmd = args["command"].as_str().unwrap_or("");
    // Show the command itself if multiline
    if cmd.contains('\n') {
        for line in cmd.lines().take(4) {
            lines.push(Line::from(Span::styled(
                format!("     {}", line),
                Style::default().fg(Color::Rgb(160, 160, 170)),
            )));
        }
    }
}

// ── write_file content ──────────────────────────────────────────────
fn render_write_content(
    lines: &mut Vec<Line<'static>>,
    args: &serde_json::Value,
    _is_error: bool,
) {
    let content = args["content"].as_str().unwrap_or("");
    let line_count = content.lines().count();
    if line_count > 0 {
        for line in content.lines().take(5) {
            lines.push(Line::from(Span::styled(
                format!("     + {}", line),
                Style::default().fg(GREEN),
            )));
        }
        if line_count > 5 {
            lines.push(Line::from(Span::styled(
                format!("     … {} more lines", line_count - 5),
                Style::default().fg(DIM),
            )));
        }
    }
}

// ── edit_file content ───────────────────────────────────────────────
fn render_edit_content(
    lines: &mut Vec<Line<'static>>,
    args: &serde_json::Value,
    _is_error: bool,
) {
    let old = args["old_string"].as_str().unwrap_or("");
    let new = args["new_string"].as_str().unwrap_or("");
    let max_diff = 5;

    for (i, line) in old.lines().enumerate() {
        if i >= max_diff {
            lines.push(Line::from(Span::styled(
                format!("     … {} more removed", old.lines().count() - max_diff),
                Style::default().fg(RED),
            )));
            break;
        }
        lines.push(Line::from(Span::styled(
            format!("     - {}", line),
            Style::default().fg(RED),
        )));
    }
    for (i, line) in new.lines().enumerate() {
        if i >= max_diff {
            lines.push(Line::from(Span::styled(
                format!("     … {} more added", new.lines().count() - max_diff),
                Style::default().fg(GREEN),
            )));
            break;
        }
        lines.push(Line::from(Span::styled(
            format!("     + {}", line),
            Style::default().fg(GREEN),
        )));
    }
}

// ── Standalone tool result (no preceding ToolCall) ──────────────────
pub fn render_standalone_result(lines: &mut Vec<Line<'static>>, entry: &ChatEntry) {
    if let ChatRole::ToolResult { name, is_error } = &entry.role {
        let color = if *is_error { RED } else { DIM };
        lines.push(Line::from(vec![
            Span::styled("  ⎿ ", Style::default().fg(ACCENT)),
            Span::styled(name.clone(), Style::default().fg(WHITE).add_modifier(Modifier::BOLD)),
        ]));
        for (i, line) in entry.content.lines().enumerate() {
            if i >= 5 {
                lines.push(Line::from(Span::styled(
                    format!("     … {} more lines", entry.content.lines().count() - 5),
                    Style::default().fg(DIM),
                )));
                break;
            }
            lines.push(Line::from(Span::styled(
                format!("     {}", line),
                Style::default().fg(color),
            )));
        }
    }
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
        let total_w: usize = line.spans.iter()
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

// ── Header (two-column: info left, gradient logo right) ─────────────
pub fn render_header(app: &App, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let border = Style::default().fg(DIM);
    let title_style = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let info = Style::default().fg(Color::Rgb(180, 210, 255)); // light blue for info
    let dim = Style::default().fg(DIM);
    let accent = Style::default().fg(ACCENT);
    let green = Style::default().fg(GREEN);

    let box_w = width.saturating_sub(4).min(100);
    let inner_w = box_w.saturating_sub(3); // "│ " ... "│"

    let session_short = if app.session.session_id.len() >= 8 {
        app.session.session_id[..8].to_string()
    } else {
        app.session.session_id.clone()
    };
    let model = app.session.model.clone();
    let workdir = app.session.working_dir.clone();

    // Logo lines (each exactly 34 display cols)
    let logo = [
        "██╗   ██╗ ██████╗ ██████╗ ███████╗",
        "╚██╗ ██╔╝██╔═══██╗██╔══██╗██╔════╝",
        " ╚████╔╝ ██║   ██║██║  ██║█████╗  ",
        "  ╚██╔╝  ██║   ██║██║  ██║██╔══╝  ",
        "   ██║   ╚██████╔╝██████╔╝███████╗",
        "   ╚═╝    ╚═════╝ ╚═════╝ ╚══════╝",
    ];
    let logo_w = 34usize;
    // Gradient purple colors for each logo row
    let logo_colors = [
        Color::Rgb(120, 80, 255),
        Color::Rgb(140, 100, 255),
        Color::Rgb(155, 115, 255),
        Color::Rgb(170, 130, 255),
        Color::Rgb(185, 150, 255),
        Color::Rgb(200, 170, 255),
    ];

    let show_logo = inner_w > logo_w + 30; // need at least 30 cols for left side

    // Helper: build a row with left content + right logo (or just left if narrow)
    let make_row = |left_spans: Vec<Span<'static>>, logo_idx: Option<usize>| -> Line<'static> {
        let left_w: usize = left_spans.iter()
            .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
            .sum();

        let mut spans = vec![Span::styled("│ ", border)];
        spans.extend(left_spans);

        if show_logo {
            if let Some(idx) = logo_idx {
                let gap = inner_w.saturating_sub(left_w + logo_w);
                spans.push(Span::raw(" ".repeat(gap)));
                spans.push(Span::styled(
                    logo[idx].to_string(),
                    Style::default().fg(logo_colors[idx]).add_modifier(Modifier::BOLD),
                ));
            } else {
                let pad = inner_w.saturating_sub(left_w);
                spans.push(Span::raw(" ".repeat(pad)));
            }
        } else {
            let pad = inner_w.saturating_sub(left_w);
            spans.push(Span::raw(" ".repeat(pad)));
        }

        spans.push(Span::raw(" "));
        spans.push(Span::styled("│", border));
        Line::from(spans)
    };

    // ╭─── Yode v0.1 ─────...──╮
    let title = " Yode v0.1 ";
    let rule_right = box_w.saturating_sub(title.len() + 3); // 3 for "╭──"
    lines.push(Line::from(vec![
        Span::styled("╭──", border),
        Span::styled(title, title_style),
        Span::styled("─".repeat(rule_right), border),
        Span::styled("╮", border),
    ]));

    // Row 0: empty + logo[0]
    lines.push(make_row(vec![], Some(0)));

    // Row 1: model + logo[1]
    lines.push(make_row(vec![
        Span::styled("  ", Style::default()),
        Span::styled(model, info),
    ], Some(1)));

    // Row 2: workdir + logo[2]
    lines.push(make_row(vec![
        Span::styled("  ", Style::default()),
        Span::styled(workdir, green),
    ], Some(2)));

    // Row 3: session + logo[3]
    lines.push(make_row(vec![
        Span::styled("  ", Style::default()),
        Span::styled(format!("session {}", session_short), dim),
    ], Some(3)));

    // Row 4: empty + logo[4]
    lines.push(make_row(vec![], Some(4)));

    // Row 5: tips + logo[5]
    lines.push(make_row(vec![
        Span::styled("  ", Style::default()),
        Span::styled("? ", accent),
        Span::styled("/help · /keys · Shift+Tab mode · Ctrl+C×2 quit", dim),
    ], Some(5)));

    // ╰─────...─╯
    lines.push(Line::from(vec![
        Span::styled("╰", border),
        Span::styled("─".repeat(box_w.saturating_sub(2)), border),
        Span::styled("╯", border),
    ]));

    lines
}

// ── Markdown Renderer ───────────────────────────────────────────────
pub fn render_markdown(text: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut code_block_lines: Vec<String> = Vec::new();
    let mut in_table = false;
    let mut table_rows: Vec<Vec<String>> = Vec::new();

    let raw_lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < raw_lines.len() {
        let raw = raw_lines[i];

        // Code blocks
        if raw.starts_with("```") {
            if in_code_block {
                // End code block — render accumulated lines
                for cl in &code_block_lines {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", cl),
                        Style::default().fg(WHITE).bg(CODE_BG),
                    )));
                }
                code_block_lines.clear();
                in_code_block = false;
            } else {
                // Flush any pending table
                if in_table {
                    render_table(&mut lines, &table_rows);
                    table_rows.clear();
                    in_table = false;
                }
                let lang = raw.trim_start_matches('`').trim();
                if !lang.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("  ┌─ {} ─", lang),
                        Style::default().fg(DIM),
                    )));
                } else {
                    lines.push(Line::from(Span::styled(
                        "  ┌──────",
                        Style::default().fg(DIM),
                    )));
                }
                in_code_block = true;
            }
            i += 1;
            continue;
        }
        if in_code_block {
            code_block_lines.push(raw.to_string());
            i += 1;
            continue;
        }

        // Table detection: lines containing | characters
        if raw.contains('|') && raw.trim().starts_with('|') {
            let trimmed = raw.trim();
            // Check for separator row (|---|---|)
            if trimmed.chars().all(|c| c == '|' || c == '-' || c == ':' || c == ' ') {
                // Table separator — just mark we're in a table, skip this row
                in_table = true;
                i += 1;
                continue;
            }
            // Parse table cells
            let cells: Vec<String> = trimmed.split('|')
                .filter(|s| !s.is_empty())
                .map(|s| s.trim().to_string())
                .collect();
            if !cells.is_empty() {
                table_rows.push(cells);
                in_table = true;
            }
            i += 1;
            continue;
        } else if in_table {
            // End of table
            render_table(&mut lines, &table_rows);
            table_rows.clear();
            in_table = false;
            // Fall through to process current line
        }

        // Horizontal rule
        let trimmed = raw.trim();
        if (trimmed.starts_with("---") || trimmed.starts_with("***") || trimmed.starts_with("___"))
            && trimmed.chars().all(|c| c == '-' || c == '*' || c == '_' || c == ' ')
            && trimmed.len() >= 3
        {
            lines.push(Line::from(Span::styled(
                "  ────────────────────────────────────────",
                Style::default().fg(DIM),
            )));
            i += 1;
            continue;
        }

        // Headers
        if raw.starts_with("### ") {
            lines.push(Line::from(Span::styled(
                format!("  ### {}", &raw[4..]),
                Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
            )));
            i += 1;
            continue;
        }
        if raw.starts_with("## ") {
            lines.push(Line::from(Span::styled(
                format!("  ## {}", &raw[3..]),
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
            )));
            i += 1;
            continue;
        }
        if raw.starts_with("# ") {
            lines.push(Line::from(Span::styled(
                format!("  # {}", &raw[2..]),
                Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
            )));
            i += 1;
            continue;
        }

        // Blockquotes
        if raw.starts_with("> ") || raw == ">" {
            let content = if raw.len() > 2 { &raw[2..] } else { "" };
            let mut spans = vec![
                Span::styled("  ▎ ", Style::default().fg(Color::Rgb(100, 100, 100))),
            ];
            spans.extend(parse_inline(content.to_string()));
            lines.push(Line::from(spans));
            i += 1;
            continue;
        }

        // Task lists
        if raw.starts_with("- [x] ") || raw.starts_with("- [X] ") {
            let content = &raw[6..];
            let mut spans = vec![
                Span::styled("  ☑ ", Style::default().fg(GREEN)),
            ];
            spans.extend(parse_inline(content.to_string()));
            lines.push(Line::from(spans));
            i += 1;
            continue;
        }
        if raw.starts_with("- [ ] ") {
            let content = &raw[6..];
            let mut spans = vec![
                Span::styled("  ☐ ", Style::default().fg(DIM)),
            ];
            spans.extend(parse_inline(content.to_string()));
            lines.push(Line::from(spans));
            i += 1;
            continue;
        }

        // Unordered lists (with indentation support)
        if raw.starts_with("- ") || raw.starts_with("* ") {
            let mut spans = vec![Span::styled("  • ", Style::default().fg(DIM))];
            spans.extend(parse_inline(raw[2..].to_string()));
            lines.push(Line::from(spans));
            i += 1;
            continue;
        }
        // Indented sub-items
        if raw.starts_with("  - ") || raw.starts_with("  * ") {
            let mut spans = vec![Span::styled("    ◦ ", Style::default().fg(DIM))];
            spans.extend(parse_inline(raw.trim_start()[2..].to_string()));
            lines.push(Line::from(spans));
            i += 1;
            continue;
        }
        if raw.starts_with("    - ") || raw.starts_with("    * ") {
            let mut spans = vec![Span::styled("      ▪ ", Style::default().fg(DIM))];
            spans.extend(parse_inline(raw.trim_start()[2..].to_string()));
            lines.push(Line::from(spans));
            i += 1;
            continue;
        }

        // Numbered lists
        if let Some((num, rest)) = try_numbered_list(raw) {
            let mut spans = vec![Span::styled(format!("  {}. ", num), Style::default().fg(DIM))];
            spans.extend(parse_inline(rest.to_string()));
            lines.push(Line::from(spans));
            i += 1;
            continue;
        }

        // Regular paragraph
        lines.push(Line::from(parse_inline(raw.to_string())));
        i += 1;
    }

    // Flush remaining
    if in_code_block {
        for cl in &code_block_lines {
            lines.push(Line::from(Span::styled(
                format!("  {}", cl),
                Style::default().fg(WHITE).bg(CODE_BG),
            )));
        }
        lines.push(Line::from(Span::styled("  └──────", Style::default().fg(DIM))));
    }
    if in_table && !table_rows.is_empty() {
        render_table(&mut lines, &table_rows);
    }
    lines
}

/// Render a markdown table with aligned columns.
fn render_table(lines: &mut Vec<Line<'static>>, rows: &[Vec<String>]) {
    if rows.is_empty() { return; }

    // Calculate column widths
    let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut widths = vec![0usize; col_count];
    for row in rows {
        for (j, cell) in row.iter().enumerate() {
            if j < col_count {
                widths[j] = widths[j].max(cell.len());
            }
        }
    }

    // Cap column widths at 30
    for w in &mut widths {
        *w = (*w).min(30);
    }

    // Render header row (first row)
    if let Some(header) = rows.first() {
        let mut spans = vec![Span::styled("  ", Style::default())];
        for (j, cell) in header.iter().enumerate() {
            let w = widths.get(j).copied().unwrap_or(10);
            spans.push(Span::styled(
                format!(" {:<w$} ", cell, w = w),
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            ));
            if j < header.len() - 1 {
                spans.push(Span::styled("│", Style::default().fg(DIM)));
            }
        }
        lines.push(Line::from(spans));

        // Separator
        let sep: String = widths.iter()
            .map(|w| "─".repeat(w + 2))
            .collect::<Vec<_>>()
            .join("┼");
        lines.push(Line::from(Span::styled(format!("  {}", sep), Style::default().fg(DIM))));
    }

    // Data rows
    for row in rows.iter().skip(1) {
        let mut spans = vec![Span::styled("  ", Style::default())];
        for (j, cell) in row.iter().enumerate() {
            let w = widths.get(j).copied().unwrap_or(10);
            spans.push(Span::styled(
                format!(" {:<w$} ", cell, w = w),
                Style::default().fg(WHITE),
            ));
            if j < row.len() - 1 {
                spans.push(Span::styled("│", Style::default().fg(DIM)));
            }
        }
        lines.push(Line::from(spans));
    }
}

fn try_numbered_list(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_start();
    let dot = trimmed.find(". ")?;
    let num = &trimmed[..dot];
    if num.len() <= 3 && num.chars().all(|c| c.is_ascii_digit()) {
        Some((num, &trimmed[dot + 2..]))
    } else {
        None
    }
}

fn parse_inline(text: String) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut remaining: &str = &text;

    while !remaining.is_empty() {
        if let Some(pos) = remaining.find("**") {
            if pos > 0 { spans.push(Span::raw(remaining[..pos].to_string())); }
            remaining = &remaining[pos + 2..];
            if let Some(end) = remaining.find("**") {
                spans.push(Span::styled(remaining[..end].to_string(),
                    Style::default().add_modifier(Modifier::BOLD)));
                remaining = &remaining[end + 2..];
            } else {
                spans.push(Span::raw("**".to_string()));
            }
        } else if let Some(pos) = remaining.find('`') {
            if pos > 0 { spans.push(Span::raw(remaining[..pos].to_string())); }
            remaining = &remaining[pos + 1..];
            if let Some(end) = remaining.find('`') {
                spans.push(Span::styled(remaining[..end].to_string(),
                    Style::default().fg(YELLOW).bg(INLINE_CODE_BG)));
                remaining = &remaining[end + 1..];
            } else {
                spans.push(Span::raw("`".to_string()));
            }
        } else {
            spans.push(Span::raw(remaining.to_string()));
            break;
        }
    }
    if spans.is_empty() { spans.push(Span::raw(String::new())); }
    spans
}

// ── Helpers ─────────────────────────────────────────────────────────
fn truncate_str(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max])
    } else {
        s.to_string()
    }
}

fn shorten_path(path: &str) -> String {
    // Show only the last 2 components
    let parts: Vec<&str> = path.rsplitn(3, '/').collect();
    if parts.len() >= 3 {
        format!(".../{}/{}", parts[1], parts[0])
    } else {
        path.to_string()
    }
}
