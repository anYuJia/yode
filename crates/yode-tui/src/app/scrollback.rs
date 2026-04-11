mod entry_formatting;

use std::io::{self, Write as IoWrite};
use std::time::Duration;

use anyhow::Result;
use crossterm::terminal::{Clear, ClearType};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::style::{Color, Modifier};
use ratatui::Terminal;

use self::entry_formatting::{format_entry_as_strings, md_line_color};
use super::rendering::{highlight_code_line, is_code_block_line, process_md_line, strip_ansi};
use super::{App, ChatRole};
use crate::ui::chat_layout::render_header;

/// Print lines to terminal scrollback.
fn raw_print_lines(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    lines: &[(String, Option<crossterm::style::Color>, bool)],
) -> Result<()> {
    if lines.is_empty() {
        return Ok(());
    }

    let term_width = crossterm::terminal::size()?.0 as usize;
    let actual_rows: usize = lines
        .iter()
        .map(|(text, _color, _)| {
            let visible = if text.contains('\x1b') {
                unicode_width::UnicodeWidthStr::width(strip_ansi(text).as_str())
            } else {
                unicode_width::UnicodeWidthStr::width(text.as_str())
            };
            if visible == 0 || term_width == 0 {
                1
            } else {
                visible.div_ceil(term_width).max(1)
            }
        })
        .sum();

    terminal.insert_before(actual_rows as u16, |_buf| {})?;
    let backend = terminal.backend_mut();
    crossterm::queue!(backend, crossterm::cursor::MoveUp(actual_rows as u16),)?;

    for (text, color, bold) in lines {
        crossterm::queue!(backend, crossterm::cursor::MoveToColumn(0))?;
        crossterm::queue!(
            backend,
            crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine)
        )?;
        if *bold {
            crossterm::queue!(
                backend,
                crossterm::style::SetAttribute(crossterm::style::Attribute::Bold)
            )?;
        }
        if let Some(c) = color {
            crossterm::queue!(backend, crossterm::style::SetForegroundColor(*c))?;
        }
        crossterm::queue!(backend, crossterm::style::Print(text))?;
        crossterm::queue!(backend, crossterm::style::ResetColor)?;
        if *bold {
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

/// Format a duration as human-readable string.
pub(crate) fn format_duration(d: Duration) -> String {
    let total_secs = d.as_secs();
    if total_secs >= 60 {
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        if secs == 0 {
            format!("{}m", mins)
        } else {
            format!("{}m {}s", mins, secs)
        }
    } else {
        format!("{}s", total_secs)
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
    for i in 0..app.chat_entries.len() {
        let entry = &app.chat_entries[i];
        let text_lines = format_entry_as_strings(entry, &app.chat_entries, i);

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
    }
    app.printed_count = app.chat_entries.len();
    stdout.flush()?;
    Ok(())
}

pub(super) fn flush_entries_to_scrollback(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let mut all_output: Vec<(String, Option<crossterm::style::Color>, bool)> = Vec::new();

    if !app.streaming_buf.is_empty() {
        let complete_count = app.streaming_buf.matches('\n').count();
        if complete_count > app.streaming_printed_lines {
            let all_lines: Vec<&str> = app.streaming_buf.lines().collect();
            let to_print =
                &all_lines[app.streaming_printed_lines..complete_count.min(all_lines.len())];

            let needs_spacer = app.streaming_printed_lines == 0;
            let mut first_printed = app.streaming_printed_lines > 0;
            let mut lines_printed_in_this_batch = 0;
            for raw_text in to_print.iter() {
                if raw_text.trim().is_empty() {
                    lines_printed_in_this_batch += 1;
                    continue;
                }

                if !first_printed && raw_text.trim().is_empty() {
                    lines_printed_in_this_batch += 1;
                    continue;
                }
                let is_first = !first_printed;
                if is_first && needs_spacer {
                    all_output.push((String::new(), None, false));
                }
                let text = process_md_line(raw_text, &mut app.streaming_in_code_block);
                let prefix = if is_first { "⏺ " } else { "  " };
                if is_first {
                    let color = crossterm::style::Color::White;
                    all_output.push((format!("{}{}", prefix, text), Some(color), false));
                    first_printed = true;
                } else if is_code_block_line(&text) {
                    let highlighted = highlight_code_line(&text);
                    all_output.push((format!("{}{}", prefix, highlighted), None, false));
                } else {
                    let (color, bold) = md_line_color(&text);
                    let color_opt = if matches!(color, crossterm::style::Color::Reset) {
                        None
                    } else {
                        Some(color)
                    };
                    all_output.push((format!("{}{}", prefix, text), color_opt, bold));
                }
                lines_printed_in_this_batch += 1;
            }
            app.streaming_printed_lines += lines_printed_in_this_batch;
        }
    }

    if let Some((remainder, is_first)) = app.streaming_remainder.take() {
        let has_content = remainder.iter().any(|l| !l.trim().is_empty());
        if has_content {
            let white = crossterm::style::Color::White;
            let mut first_done = !is_first;
            for line in remainder.iter() {
                if !first_done && line.trim().is_empty() {
                    continue;
                }
                let text = process_md_line(line, &mut app.streaming_in_code_block);
                if !first_done {
                    all_output.push((String::new(), None, false));
                    all_output.push((format!("⏺ {}", text), Some(white), false));
                    first_done = true;
                } else if is_code_block_line(&text) {
                    let highlighted = highlight_code_line(&text);
                    all_output.push((format!("  {}", highlighted), None, false));
                } else {
                    let (color, bold) = md_line_color(&text);
                    let color_opt = if matches!(color, crossterm::style::Color::Reset) {
                        None
                    } else {
                        Some(color)
                    };
                    all_output.push((format!("  {}", text), color_opt, bold));
                }
            }
        }
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

        let text_lines = format_entry_as_strings(entry, &app.chat_entries, app.printed_count);
        let needs_spacer = matches!(entry.role, ChatRole::User) && app.printed_count > 0;

        if needs_spacer {
            all_output.push((String::new(), None, false));
        }
        for (text, style) in &text_lines {
            let color = style.fg.map(to_crossterm_color);
            let bold = style.add_modifier.contains(Modifier::BOLD);
            all_output.push((text.clone(), color, bold));
        }

        app.printed_count += 1;
    }

    if !all_output.is_empty() {
        raw_print_lines(terminal, &all_output)?;
    }

    Ok(())
}
