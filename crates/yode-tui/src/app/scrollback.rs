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
use super::{App, ChatRole};
use crate::tool_grouping::{
    detect_groupable_subagent_batch, detect_groupable_system_batch, detect_groupable_tool_batch,
};
use crate::ui::chat::{
    render_markdown_ansi_white_with_options, render_markdown_white_with_options,
    streaming_markdown_advance_stable_boundary,
};
use crate::ui::chat_layout::{render_header, wrap_terminal_text};

/// Print lines to terminal scrollback.
fn raw_print_lines(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    lines: &[(String, Option<crossterm::style::Color>, bool)],
) -> Result<()> {
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
    let actual_rows = wrapped_lines.len();

    terminal.insert_before(actual_rows as u16, |_buf| {})?;
    let backend = terminal.backend_mut();
    crossterm::queue!(backend, crossterm::cursor::MoveUp(actual_rows as u16),)?;

    for (text, color, bold) in wrapped_lines {
        crossterm::queue!(backend, crossterm::cursor::MoveToColumn(0))?;
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
        crossterm::queue!(backend, crossterm::cursor::MoveToNextLine(1))?;
    }

    backend.flush()?;
    Ok(())
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
            if let Some(batch) = detect_groupable_tool_batch(&app.chat_entries, i) {
                (
                    format_grouped_tool_batch(&app.chat_entries, &batch),
                    batch.next_index,
                )
            } else if let Some(batch) = detect_groupable_subagent_batch(&app.chat_entries, i) {
                (
                    format_grouped_subagent_batch(&app.chat_entries, &batch),
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
        let stable_end = streaming_markdown_advance_stable_boundary(
            &app.streaming_buf,
            app.streaming_markdown_stable_len,
        );
        if stable_end > app.streaming_markdown_stable_len {
            let new_stable = &app.streaming_buf[app.streaming_markdown_stable_len..stable_end];
            let rendered = render_markdown_ansi_white_with_options(
                new_stable,
                Some(render_width),
                app.terminal_caps.supports_hyperlinks(),
            );
            let needs_spacer = app.streaming_markdown_stable_len == 0;
            let mut first_printed = app.streaming_markdown_stable_len > 0;

            push_streaming_rendered_lines(
                &mut all_output,
                rendered,
                needs_spacer,
                &mut first_printed,
            );
            app.streaming_markdown_stable_len = stable_end;
            dirty = true;
        }

        let unstable = &app.streaming_buf[app.streaming_markdown_stable_len..];
        let next_preview_source = unstable.to_string();
        let preview_source_changed = app.streaming_markdown_preview_source != next_preview_source;
        let preview_width_changed = app.streaming_markdown_cached_width != render_width;
        if preview_source_changed || preview_width_changed {
            let next_preview = if unstable.trim().is_empty() {
                Vec::new()
            } else {
                render_markdown_white_with_options(
                    unstable,
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
        push_streaming_rendered_lines(&mut all_output, remainder, false, &mut first_done);
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
            detect_groupable_tool_batch(&app.chat_entries, app.printed_count)
        {
            (
                format_grouped_tool_batch(&app.chat_entries, &batch),
                batch.next_index,
            )
        } else if let Some(batch) =
            detect_groupable_subagent_batch(&app.chat_entries, app.printed_count)
        {
            (
                format_grouped_subagent_batch(&app.chat_entries, &batch),
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
) {
    for line in rendered {
        if line.trim().is_empty() {
            all_output.push((String::new(), None, false));
            continue;
        }
        if !*first_printed && needs_spacer {
            all_output.push((String::new(), None, false));
        }
        let prefix = if *first_printed { "  " } else { "⏺ " };
        all_output.push((format!("{}{}", prefix, line), None, false));
        *first_printed = true;
    }
}

#[cfg(test)]
mod tests {
    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};

    use super::{merge_preview_lines, push_streaming_rendered_lines, scrollback_rows_for_line};

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
        );
        assert_eq!(output[0].0, String::new());
        assert_eq!(output[1].0, "⏺ Heading");
        assert_eq!(output[2].0, String::new());
        assert_eq!(output[3].0, "  Body");
    }

    #[test]
    fn scrollback_row_count_handles_ansi_and_cjk_without_width_inflation() {
        let styled = "\x1b[97m⏺ Yode vs Claude Code 综合对比\x1b[0m";
        assert_eq!(scrollback_rows_for_line(styled, 80), 1);
    }
}
