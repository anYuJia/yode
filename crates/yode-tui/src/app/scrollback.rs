pub(crate) mod entry_formatting;

use std::io::{self, Write as IoWrite};

use anyhow::Result;
use crossterm::terminal::{Clear, ClearType};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::style::{Color, Modifier};
use ratatui::text::Line;
use ratatui::Terminal;

use self::entry_formatting::{
    format_entry_as_strings, format_grouped_subagent_batch, format_grouped_system_batch,
    format_grouped_tool_batch,
};
use super::{App, ChatEntry, ChatRole};
use crate::tool_grouping::{
    detect_groupable_subagent_batch, detect_groupable_system_batch, detect_groupable_tool_batch,
    is_agent_tool_name,
};
use crate::ui::chat::{
    render_markdown_white_with_options, streaming_markdown_advance_stable_boundary,
};
use crate::ui::chat_entries::compact_assistant_streaming_preview_markdown;
use crate::ui::chat_layout::{render_header, wrap_terminal_text};

/// Print lines to terminal scrollback.
fn raw_print_lines(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    lines: &[(String, Option<crossterm::style::Color>, bool)],
) -> Result<()> {
    if lines.is_empty() {
        return Ok(());
    }

    let lines = normalize_scrollback_print_lines(lines);
    if lines.is_empty() {
        return Ok(());
    }

    let term_width = crossterm::terminal::size()?.0 as usize;
    let wrapped_lines = lines
        .iter()
        .flat_map(|(text, color, bold)| {
            wrap_terminal_text(text, term_width)
                .into_iter()
                .map(move |wrapped| (wrapped, *color, *bold))
        })
        .collect::<Vec<_>>();
    let (_, screen_height) = crossterm::terminal::size()?;
    let viewport_area = terminal.get_frame().area();
    let scroll_region_height = if viewport_area.y == 0 {
        screen_height
    } else {
        viewport_area.y
    };
    if scroll_region_height == 0 {
        return Ok(());
    }

    let backend = terminal.backend_mut();
    write!(backend, "\x1b[1;{}r", scroll_region_height)?;

    for (text, color, bold) in wrapped_lines {
        write!(backend, "\x1b[1S")?;
        crossterm::queue!(
            backend,
            crossterm::cursor::MoveTo(0, scroll_region_height - 1),
        )?;
        crossterm::queue!(
            backend,
            crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine)
        )?;
        if bold {
            crossterm::queue!(
                backend,
                crossterm::style::SetAttribute(crossterm::style::Attribute::Bold)
            )?;
        }
        if let Some(c) = color {
            crossterm::queue!(backend, crossterm::style::SetForegroundColor(c))?;
        }
        crossterm::queue!(backend, crossterm::style::Print(text))?;
        crossterm::queue!(backend, crossterm::style::ResetColor)?;
        if bold {
            crossterm::queue!(
                backend,
                crossterm::style::SetAttribute(crossterm::style::Attribute::NoBold)
            )?;
        }
    }

    write!(backend, "\x1b[r")?;
    crossterm::queue!(
        backend,
        crossterm::cursor::MoveTo(0, screen_height.saturating_sub(1)),
    )?;
    IoWrite::flush(backend)?;
    Ok(())
}

fn normalize_scrollback_print_lines(
    lines: &[(String, Option<crossterm::style::Color>, bool)],
) -> Vec<(String, Option<crossterm::style::Color>, bool)> {
    let mut normalized = Vec::with_capacity(lines.len());
    let mut previous_blank = false;

    for (text, color, bold) in lines {
        let blank = text.trim().is_empty();
        if blank {
            if previous_blank {
                continue;
            }
            normalized.push((String::new(), None, false));
            previous_blank = true;
            continue;
        }
        normalized.push((text.clone(), *color, *bold));
        previous_blank = false;
    }

    while normalized
        .last()
        .is_some_and(|(text, _, _)| text.trim().is_empty())
    {
        normalized.pop();
    }
    normalized
}

#[cfg(test)]
fn scrollback_rows_for_line(text: &str, term_width: usize) -> usize {
    let visible = crate::ui::chat_layout::visible_text_width(text);
    if visible == 0 || term_width == 0 {
        1
    } else {
        visible.div_ceil(term_width).max(1)
    }
}

/// Convert ratatui Color to crossterm Color (handles Rgb, Indexed, and named colors).
fn to_crossterm_color(color: Color) -> crossterm::style::Color {
    match color {
        Color::Rgb(r, g, b) => crossterm::style::Color::Rgb { r, g, b },
        Color::Indexed(i) => crossterm::style::Color::AnsiValue(i),
        Color::Black => crossterm::style::Color::Black,
        Color::Red => crossterm::style::Color::Red,
        Color::Green => crossterm::style::Color::Green,
        Color::Yellow => crossterm::style::Color::Yellow,
        Color::Blue => crossterm::style::Color::Blue,
        Color::Magenta => crossterm::style::Color::Magenta,
        Color::Cyan => crossterm::style::Color::Cyan,
        Color::Gray => crossterm::style::Color::Grey,
        Color::DarkGray => crossterm::style::Color::DarkGrey,
        Color::LightRed => crossterm::style::Color::DarkRed,
        Color::LightGreen => crossterm::style::Color::DarkGreen,
        Color::LightBlue => crossterm::style::Color::DarkBlue,
        Color::LightYellow => crossterm::style::Color::DarkYellow,
        Color::LightMagenta => crossterm::style::Color::DarkMagenta,
        Color::LightCyan => crossterm::style::Color::DarkCyan,
        Color::White => crossterm::style::Color::White,
        _ => crossterm::style::Color::White,
    }
}

/// Print the welcome header into terminal stdout before starting TUI.
pub(super) fn print_header_to_stdout(app: &App) -> Result<()> {
    let width = crossterm::terminal::size()?.0 as usize;
    let header_lines = render_header(app, width);

    let mut stdout = io::stdout();
    stdout.execute(Clear(ClearType::CurrentLine))?;

    for line in header_lines {
        for span in line.spans {
            if let Some(color) = span.style.fg {
                let c = to_crossterm_color(color);
                stdout.execute(crossterm::style::SetForegroundColor(c))?;
            }
            if span.style.add_modifier.contains(Modifier::BOLD) {
                stdout.execute(crossterm::style::SetAttribute(
                    crossterm::style::Attribute::Bold,
                ))?;
            }
            stdout.execute(crossterm::style::Print(&span.content))?;
            stdout.execute(crossterm::style::SetAttribute(
                crossterm::style::Attribute::Reset,
            ))?;
        }
        stdout.execute(crossterm::style::Print("\r\n"))?;
    }
    stdout.execute(crossterm::style::SetAttribute(
        crossterm::style::Attribute::Reset,
    ))?;
    stdout.execute(crossterm::style::ResetColor)?;
    stdout.flush()?;
    Ok(())
}

pub(super) fn print_entries_to_stdout(app: &mut App) -> Result<()> {
    if app.chat_entries.is_empty() {
        return Ok(());
    }

    let mut stdout = io::stdout();
    let mut i = 0;
    while i < app.chat_entries.len() {
        let entry = &app.chat_entries[i];
        let (text_lines, next_index) =
            if let Some(batch) = detect_groupable_subagent_batch(&app.chat_entries, i) {
                (
                    format_grouped_subagent_batch(&app.chat_entries, &batch),
                    batch.next_index,
                )
            } else if let Some(batch) = detect_groupable_tool_batch(&app.chat_entries, i) {
                (
                    format_grouped_tool_batch(&app.chat_entries, &batch),
                    batch.next_index,
                )
            } else if let Some(batch) = detect_groupable_system_batch(&app.chat_entries, i) {
                (
                    format_grouped_system_batch(&app.chat_entries, &batch),
                    batch.next_index,
                )
            } else {
                (format_entry_as_strings(entry, &app.chat_entries, i), i + 1)
            };

        if i > 0 && matches!(entry.role, ChatRole::User) {
            stdout.execute(crossterm::style::Print("\r\n"))?;
        }

        for (text, style) in text_lines {
            if let Some(color) = style.fg {
                let c = to_crossterm_color(color);
                stdout.execute(crossterm::style::SetForegroundColor(c))?;
            }
            if style.add_modifier.contains(Modifier::BOLD) {
                stdout.execute(crossterm::style::SetAttribute(
                    crossterm::style::Attribute::Bold,
                ))?;
            }
            stdout.execute(crossterm::style::Print(text))?;
            stdout.execute(crossterm::style::SetAttribute(
                crossterm::style::Attribute::Reset,
            ))?;
            stdout.execute(crossterm::style::Print("\r\n"))?;
        }
        i = next_index;
    }
    app.printed_count = app.chat_entries.len();
    stdout.flush()?;
    Ok(())
}

pub(super) fn flush_entries_to_scrollback(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<bool> {
    let mut all_output: Vec<(String, Option<crossterm::style::Color>, bool)> = Vec::new();
    let mut dirty = false;

    let render_width = terminal.get_frame().area().width.saturating_sub(2) as usize;
    if !app.streaming_buf.is_empty()
        && (app.streaming_markdown_cached_buf_len != app.streaming_buf.len()
            || app.streaming_markdown_cached_width != render_width)
    {
        let next_preview_source = streaming_assistant_preview_source(&app.streaming_buf);
        let preview_source_changed = app.streaming_markdown_preview_source != next_preview_source;
        let preview_width_changed = app.streaming_markdown_cached_width != render_width;
        if preview_source_changed || preview_width_changed {
            let next_preview = if next_preview_source.trim().is_empty() {
                Vec::new()
            } else {
                render_markdown_white_with_options(
                    &next_preview_source,
                    Some(render_width),
                    app.terminal_caps.supports_hyperlinks(),
                )
            };
            let merged_preview = merge_preview_lines(&app.streaming_markdown_preview, next_preview);
            app.streaming_markdown_preview_source = next_preview_source;
            if app.streaming_markdown_preview != merged_preview {
                app.streaming_markdown_preview = merged_preview;
                dirty = true;
            }
        }
        app.streaming_markdown_cached_buf_len = app.streaming_buf.len();
        app.streaming_markdown_cached_width = render_width;
    }

    if let Some((remainder, is_first)) = app.streaming_markdown_remainder.take() {
        let mut first_done = !is_first;
        push_streaming_rendered_lines(&mut all_output, remainder, is_first, &mut first_done, true);
    }

    while app.printed_count < app.chat_entries.len() {
        let entry = &app.chat_entries[app.printed_count];

        if entry.already_printed {
            app.printed_count += 1;
            continue;
        }

        if let ChatRole::ToolCall { id: ref tid, .. } = entry.role {
            let tool_id = tid.clone();
            let has_result = app.chat_entries[app.printed_count + 1..].iter().any(
                |e| matches!(&e.role, ChatRole::ToolResult { id: ref eid, .. } if eid == &tool_id),
            );
            if !has_result {
                break;
            }
        }

        if is_pending_groupable_agent_tool(app, app.printed_count) {
            break;
        }

        if matches!(entry.role, ChatRole::SubAgentCall { .. }) {
            let has_result = app.chat_entries[app.printed_count + 1..]
                .iter()
                .any(|e| matches!(&e.role, ChatRole::SubAgentResult));
            if !has_result {
                break;
            }
        }

        if matches!(
            entry.role,
            ChatRole::SubAgentToolCall { .. } | ChatRole::SubAgentResult
        ) {
            app.printed_count += 1;
            continue;
        }

        let (text_lines, next_index) = if let Some(batch) =
            detect_groupable_subagent_batch(&app.chat_entries, app.printed_count)
        {
            (
                format_grouped_subagent_batch(&app.chat_entries, &batch),
                batch.next_index,
            )
        } else if let Some(batch) =
            detect_groupable_tool_batch(&app.chat_entries, app.printed_count)
        {
            (
                format_grouped_tool_batch(&app.chat_entries, &batch),
                batch.next_index,
            )
        } else if let Some(batch) =
            detect_groupable_system_batch(&app.chat_entries, app.printed_count)
        {
            (
                format_grouped_system_batch(&app.chat_entries, &batch),
                batch.next_index,
            )
        } else {
            (
                format_entry_as_strings(entry, &app.chat_entries, app.printed_count),
                app.printed_count + 1,
            )
        };
        let needs_spacer = matches!(entry.role, ChatRole::User) && app.printed_count > 0;

        if needs_spacer {
            all_output.push((String::new(), None, false));
        }
        for (text, style) in &text_lines {
            let color = style.fg.map(to_crossterm_color);
            let bold = style.add_modifier.contains(Modifier::BOLD);
            all_output.push((text.clone(), color, bold));
        }

        app.printed_count = next_index;
        dirty = true;
    }

    if !all_output.is_empty() {
        raw_print_lines(terminal, &all_output)?;
        dirty = true;
    }

    Ok(dirty)
}

fn is_pending_groupable_agent_tool(app: &App, index: usize) -> bool {
    if !app.is_processing {
        return false;
    }

    let Some(ChatEntry {
        role: ChatRole::ToolCall { name, .. },
        ..
    }) = app.chat_entries.get(index)
    else {
        return false;
    };
    if !is_agent_tool_name(name) {
        return false;
    }

    detect_groupable_subagent_batch(&app.chat_entries, index).is_none()
}

fn streaming_assistant_preview_source(content: &str) -> String {
    let compacted = compact_assistant_streaming_preview_markdown(content);
    if compacted.was_compacted {
        return compacted.text;
    }

    let stable_end = streaming_markdown_advance_stable_boundary(content, 0);
    if stable_end == 0 {
        return String::new();
    }
    content[..stable_end].to_string()
}

fn merge_preview_lines(existing: &[Line<'static>], next: Vec<Line<'static>>) -> Vec<Line<'static>> {
    next.into_iter()
        .enumerate()
        .map(|(index, line)| {
            if existing.get(index) == Some(&line) {
                existing[index].clone()
            } else {
                line
            }
        })
        .collect()
}

fn push_streaming_rendered_lines(
    all_output: &mut Vec<(String, Option<crossterm::style::Color>, bool)>,
    rendered: Vec<String>,
    needs_spacer: bool,
    first_printed: &mut bool,
    collapse_repeated_blank_lines: bool,
) {
    let mut previous_blank = false;
    for line in rendered {
        if line.trim().is_empty() {
            if collapse_repeated_blank_lines && previous_blank {
                continue;
            }
            all_output.push((String::new(), None, false));
            previous_blank = true;
            continue;
        }
        if !*first_printed && needs_spacer {
            all_output.push((String::new(), None, false));
        }
        let prefix = if *first_printed { "  " } else { "⏺ " };
        all_output.push((format!("{}{}", prefix, line), None, false));
        *first_printed = true;
        previous_blank = false;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};
    use yode_llm::registry::ProviderRegistry;
    use yode_tools::registry::ToolRegistry;

    use crate::app::{App, ChatEntry, ChatRole};

    use super::{
        is_pending_groupable_agent_tool, merge_preview_lines, normalize_scrollback_print_lines,
        push_streaming_rendered_lines, scrollback_rows_for_line,
        streaming_assistant_preview_source,
    };

    fn test_app() -> App {
        App::new(
            "test-model".to_string(),
            "session-1234".to_string(),
            "/tmp".to_string(),
            "test".to_string(),
            Vec::new(),
            HashMap::new(),
            Arc::new(ProviderRegistry::new()),
            Arc::new(ToolRegistry::new()),
        )
    }

    #[test]
    fn merge_preview_lines_reuses_unchanged_lines_and_replaces_changed_ones() {
        let existing = vec![
            Line::from(vec![Span::styled("same", Style::default().fg(Color::Blue))]),
            Line::from(vec![Span::styled("old", Style::default().fg(Color::Red))]),
        ];
        let next = vec![
            Line::from(vec![Span::styled("same", Style::default().fg(Color::Blue))]),
            Line::from(vec![Span::styled("new", Style::default().fg(Color::Green))]),
        ];

        let merged = merge_preview_lines(&existing, next);
        assert_eq!(merged[0], existing[0]);
        assert_ne!(merged[1], existing[1]);
        assert_eq!(merged[1].to_string(), "new");
    }

    #[test]
    fn push_streaming_rendered_lines_preserves_blank_lines() {
        let mut output = Vec::new();
        let mut first_printed = false;
        push_streaming_rendered_lines(
            &mut output,
            vec!["Heading".to_string(), String::new(), "Body".to_string()],
            true,
            &mut first_printed,
            false,
        );
        assert_eq!(output[0].0, String::new());
        assert_eq!(output[1].0, "⏺ Heading");
        assert_eq!(output[2].0, String::new());
        assert_eq!(output[3].0, "  Body");
    }

    #[test]
    fn push_streaming_rendered_lines_separates_final_first_chunk_from_prompt() {
        let mut output = Vec::new();
        let mut first_printed = false;
        push_streaming_rendered_lines(
            &mut output,
            vec!["你好！我是 claude-opus-4-7".to_string()],
            true,
            &mut first_printed,
            false,
        );

        assert_eq!(output[0].0, String::new());
        assert_eq!(output[1].0, "⏺ 你好！我是 claude-opus-4-7");
    }

    #[test]
    fn push_streaming_rendered_lines_collapses_repeated_blank_lines_when_compacted() {
        let mut output = Vec::new();
        let mut first_printed = false;
        push_streaming_rendered_lines(
            &mut output,
            vec![
                "Heading".to_string(),
                String::new(),
                String::new(),
                "Body".to_string(),
            ],
            true,
            &mut first_printed,
            true,
        );
        assert_eq!(output[0].0, String::new());
        assert_eq!(output[1].0, "⏺ Heading");
        assert_eq!(output[2].0, String::new());
        assert_eq!(output[3].0, "  Body");
        assert_eq!(output.len(), 4);
    }

    #[test]
    fn normalize_scrollback_print_lines_collapses_blank_runs_and_trims_tail() {
        let input = vec![
            ("⏺ Read 6 files".to_string(), None, false),
            (String::new(), None, false),
            ("   ".to_string(), None, false),
            ("\t".to_string(), None, false),
            ("⏺ 已有足够信息".to_string(), None, false),
            (String::new(), None, false),
            (String::new(), None, false),
        ];

        let normalized = normalize_scrollback_print_lines(&input);
        assert_eq!(
            normalized
                .iter()
                .map(|(text, _, _)| text.as_str())
                .collect::<Vec<_>>(),
            vec!["⏺ Read 6 files", "", "⏺ 已有足够信息"]
        );
    }

    #[test]
    fn pending_agent_tool_waits_for_grouping_while_turn_is_active() {
        let mut app = test_app();
        app.is_processing = true;
        app.chat_entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "agent".to_string(),
                },
                "{\"description\":\"Analyze Yode architecture\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "a".to_string(),
                    name: "agent".to_string(),
                    is_error: false,
                },
                "background task task-1 launched".to_string(),
            ),
        ];
        assert!(is_pending_groupable_agent_tool(&app, 0));

        app.chat_entries.push(ChatEntry::new(
            ChatRole::ToolCall {
                id: "b".to_string(),
                name: "agent".to_string(),
            },
            "{\"description\":\"Find claude-code-rev project\"}".to_string(),
        ));
        assert!(!is_pending_groupable_agent_tool(&app, 0));

        app.is_processing = false;
        assert!(!is_pending_groupable_agent_tool(&app, 0));
    }

    #[test]
    fn compacted_streaming_preview_should_not_print_repeated_stable_chunks_verbatim() {
        let compacted = crate::ui::chat_entries::compact_assistant_streaming_preview_markdown(
            "Yode vs Claude Code 深度对比与优化建议\n一、基本规模\n┌────────────┬────────────┐\n│ 维度 │ Yode │\n└────────────┴────────────┘\n二、已做好的部分\n• Agent\n• TUI\n• Hooks\n三、关键差距\nP0 — 严重影响日常使用\n\n\n\n\n1. 上下文压缩",
        );
        assert!(compacted.was_compacted);
        assert!(compacted.text.contains("项目对比报告已压缩"));
        assert!(!compacted.text.contains("│ 维度 │ Yode │"));
    }

    #[test]
    fn full_stream_compaction_detects_report_even_if_first_chunk_is_short() {
        let first_chunk = "结合项目结构、代码和已有的深度分析笔记，以下是完整的对比分析：";
        let full = format!(
            "{}\n\nYode vs Claude Code 深度对比与优化建议\n一、基本规模\n┌────────────┬────────────┐\n│ 维度 │ Yode │\n└────────────┴────────────┘\n二、Yode 已经做好的部分\n• Agent\n• TUI\n• Hooks\n三、关键差距分析\nP0 — 严重影响日常使用\n1. 上下文压缩",
            first_chunk
        );
        let first =
            crate::ui::chat_entries::compact_assistant_streaming_preview_markdown(first_chunk);
        let whole = crate::ui::chat_entries::compact_assistant_streaming_preview_markdown(&full);
        assert!(!first.was_compacted);
        assert!(whole.was_compacted);
    }

    #[test]
    fn streaming_preview_waits_for_complete_line_for_normal_text() {
        assert_eq!(streaming_assistant_preview_source("partial"), "");
        assert_eq!(
            streaming_assistant_preview_source("first line\nsecond"),
            "first line\n"
        );
    }

    #[test]
    fn streaming_preview_compacts_project_report_before_complete_line_boundary() {
        let preview = streaming_assistant_preview_source(
            "Yode vs Claude Code 深度对比与优化建议\n一、基本规模\n┌────────────┬────────────┐\n│ 维度 │ Yode │\n└────────────┴────────────┘\n二、Yode 已经做好的部分\n• Agent\n• TUI\n三、关键差距分析\nP0 — 严重影响日常使用\n1. 上下文压缩：单层 vs 7 层",
        );
        assert!(preview.contains("项目对比报告已压缩"));
        assert!(!preview.contains("│ 维度 │ Yode │"));
    }

    #[test]
    fn scrollback_row_count_handles_ansi_and_cjk_without_width_inflation() {
        let styled = "\x1b[97m⏺ Yode vs Claude Code 综合对比\x1b[0m";
        assert_eq!(scrollback_rows_for_line(styled, 80), 1);
    }
}
