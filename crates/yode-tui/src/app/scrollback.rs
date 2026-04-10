use std::io::{self, Write as IoWrite};
use std::time::Duration;

use anyhow::Result;
use crossterm::terminal::{Clear, ClearType};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::style::{Color, Modifier};
use ratatui::Terminal;

use super::rendering::{
    capitalize, highlight_code_line, is_code_block_line, markdown_to_plain, process_md_line,
    strip_ansi, truncate_line,
};
use super::{App, ChatEntry, ChatRole};
use crate::ui;

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
    let header_lines = ui::chat::render_header(app, width);

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

fn md_line_color(line: &str) -> (crossterm::style::Color, bool) {
    if line.starts_with("━━ ") || line.starts_with("━━━") {
        (crossterm::style::Color::Yellow, true)
    } else if line.starts_with("▸ ") {
        (crossterm::style::Color::Blue, true)
    } else if line.starts_with("  ▹ ") {
        (crossterm::style::Color::Cyan, false)
    } else if line.starts_with("    ") && !line.trim().is_empty() {
        (crossterm::style::Color::Green, false)
    } else if line.starts_with("▎ ") {
        (crossterm::style::Color::DarkYellow, false)
    } else if line.starts_with("────") {
        (crossterm::style::Color::DarkGrey, false)
    } else if line.starts_with("── ") || line.starts_with("───") {
        (crossterm::style::Color::Cyan, true)
    } else if line.contains('│') {
        (crossterm::style::Color::White, false)
    } else {
        (crossterm::style::Color::Reset, false)
    }
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

fn format_entry_as_strings(
    entry: &ChatEntry,
    all_entries: &[ChatEntry],
    index: usize,
) -> Vec<(String, ratatui::style::Style)> {
    let mut result: Vec<(String, ratatui::style::Style)> = Vec::new();
    let dim = ratatui::style::Style::default().fg(Color::Gray);
    let accent = ratatui::style::Style::default().fg(Color::LightMagenta);
    let cyan = ratatui::style::Style::default().fg(Color::Indexed(51));
    let white = ratatui::style::Style::default().fg(Color::Indexed(231));
    let red = ratatui::style::Style::default().fg(Color::LightRed);

    match &entry.role {
        ChatRole::User => {
            let mut first = true;
            for line in entry.content.lines() {
                if first {
                    result.push((format!("> {}", line), cyan.add_modifier(Modifier::BOLD)));
                    first = false;
                } else {
                    result.push((format!("  {}", line), cyan));
                }
            }
            if first {
                result.push(("> ".to_string(), cyan.add_modifier(Modifier::BOLD)));
            }
        }
        ChatRole::Assistant => {
            result.push((String::new(), dim));
            let processed = markdown_to_plain(&entry.content);
            if processed.trim().is_empty() {
                return result;
            }
            let mut first = true;
            for line in processed.lines() {
                if line.trim().is_empty() {
                    result.push((String::new(), dim));
                    continue;
                }
                if first {
                    result.push((format!("⏺ {}", line), white));
                    first = false;
                } else if is_code_block_line(&line) {
                    let highlighted = highlight_code_line(&line);
                    result.push((
                        format!("  {}", highlighted),
                        ratatui::style::Style::default(),
                    ));
                } else {
                    result.push((format!("  {}", line), white));
                }
            }
        }
        ChatRole::ToolCall {
            id: ref tid,
            ref name,
        } => {
            let args: serde_json::Value = serde_json::from_str(&entry.content).unwrap_or_default();
            let tool_result = all_entries[index + 1..].iter().find(
                |e| matches!(&e.role, ChatRole::ToolResult { id: ref eid, .. } if eid == tid),
            );

            let timing = tool_result
                .and_then(|r| r.duration)
                .map(|d| {
                    if d.as_secs() >= 1 {
                        format!(" ── {:.1}s", d.as_secs_f64())
                    } else {
                        format!(" ── {}ms", d.as_millis())
                    }
                })
                .unwrap_or_default();

            let green = ratatui::style::Style::default().fg(Color::LightGreen);
            let red_dim = ratatui::style::Style::default().fg(Color::LightRed);

            if name == "edit_file" {
                let file_path = args["file_path"].as_str().unwrap_or("???");
                let display_path = file_path
                    .strip_prefix(&format!(
                        "{}/",
                        std::env::current_dir()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default()
                    ))
                    .unwrap_or(file_path);

                let old_str = args["old_string"].as_str().unwrap_or("");
                let new_str = args["new_string"].as_str().unwrap_or("");
                let old_lines: Vec<&str> = old_str.lines().collect();
                let new_lines: Vec<&str> = new_str.lines().collect();
                let added = new_lines.len();
                let removed = old_lines.len();

                let summary = if added > 0 && removed > 0 {
                    format!("Added {} lines, removed {} lines", added, removed)
                } else if added > 0 {
                    format!("Added {} lines", added)
                } else {
                    format!("Removed {} lines", removed)
                };

                result.push((format!("⏺ Update({}){}", display_path, timing), accent));
                result.push((format!("  ⎿  {}", summary), dim));

                let max_diff = 6;
                let mut shown = 0;
                let total = old_lines.len() + new_lines.len();
                for line in &old_lines {
                    if shown >= max_diff {
                        result.push((
                            format!("     … +{} lines (ctrl+o to expand)", total - shown),
                            dim,
                        ));
                        break;
                    }
                    result.push((format!("     - {}", line), red_dim));
                    shown += 1;
                }
                if shown < max_diff {
                    for line in &new_lines {
                        if shown >= max_diff {
                            result.push((
                                format!("     … +{} lines (ctrl+o to expand)", total - shown),
                                dim,
                            ));
                            break;
                        }
                        result.push((format!("     + {}", line), green));
                        shown += 1;
                    }
                }
            } else if name == "read_file" {
                let file_path = args["file_path"].as_str().unwrap_or("???");
                let display_path = file_path
                    .strip_prefix(&format!(
                        "{}/",
                        std::env::current_dir()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default()
                    ))
                    .unwrap_or(file_path);
                result.push((format!("⏺ Read({}){}", display_path, timing), accent));
            } else if name == "write_file" {
                let file_path = args["file_path"].as_str().unwrap_or("???");
                let display_path = file_path
                    .strip_prefix(&format!(
                        "{}/",
                        std::env::current_dir()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default()
                    ))
                    .unwrap_or(file_path);
                let content = args["content"].as_str().unwrap_or("");
                let total_lines = content.lines().count();
                result.push((format!("⏺ Write({}){}", display_path, timing), accent));
                result.push((format!("  ⎿  {} lines written", total_lines), dim));
                let max_preview = 3;
                for (i, line) in content.lines().enumerate() {
                    if i >= max_preview {
                        result.push((
                            format!(
                                "     … +{} lines (ctrl+o to expand)",
                                total_lines - max_preview
                            ),
                            dim,
                        ));
                        break;
                    }
                    result.push((format!("     + {}", line), green));
                }
            } else {
                let summary = tool_summary_str(name, &args);
                result.push((
                    format!("⏺ {}({}){}", capitalize(name), summary, timing),
                    accent,
                ));

                if let Some(res) = tool_result {
                    let max_lines = 3;
                    let max_line_chars = crossterm::terminal::size()
                        .map(|(w, _)| (w as usize).saturating_sub(10))
                        .unwrap_or(120);
                    for (i, line) in res.content.lines().enumerate() {
                        if i >= max_lines {
                            result.push((
                                format!(
                                    "     … +{} lines (ctrl+o to expand)",
                                    res.content.lines().count() - max_lines
                                ),
                                dim,
                            ));
                            break;
                        }
                        let prefix = if i == 0 { "  ⎿  " } else { "     " };
                        let style = if matches!(res.role, ChatRole::ToolResult { is_error, .. } if is_error)
                        {
                            red
                        } else {
                            dim
                        };
                        let display = truncate_line(line, max_line_chars);
                        result.push((format!("{}{}", prefix, display), style));
                    }
                }
            }
        }
        ChatRole::ToolResult { id: ref rid, .. } => {
            let has_preceding = index > 0
                && all_entries[..index].iter().rev().any(
                    |e| matches!(&e.role, ChatRole::ToolCall { id: ref tid, .. } if tid == rid),
                );
            if !has_preceding {
                result.push((
                    format!("  ⎿ {}", entry.content.lines().next().unwrap_or("")),
                    dim,
                ));
            }
        }
        ChatRole::Error => {
            let err_style = ratatui::style::Style::default()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD);
            result.push(("╭─ Error ──────────────────────────".to_string(), err_style));
            for line in entry.content.lines() {
                result.push((format!("│ {}", line), red));
            }
            result.push(("╰──────────────────────────────────".to_string(), err_style));
        }
        ChatRole::System => {
            if entry.content.is_empty() {
                result.push((String::new(), dim));
            } else {
                for line in entry.content.lines() {
                    result.push((format!("  {}", line), dim));
                }
            }
        }
        ChatRole::SubAgentCall { description } => {
            let mut sub_tools: Vec<String> = Vec::new();
            let mut agent_duration: Option<Duration> = None;
            for e in &all_entries[index + 1..] {
                match &e.role {
                    ChatRole::SubAgentToolCall { name } => {
                        sub_tools.push(name.clone());
                    }
                    ChatRole::SubAgentResult => {
                        agent_duration = e.duration;
                        break;
                    }
                    _ => break,
                }
            }

            let agent_type = if description.to_lowercase().contains("explore") {
                "Explore"
            } else if description.to_lowercase().contains("plan") {
                "Plan"
            } else {
                "Agent"
            };

            let timing = agent_duration
                .map(|d| format!(" ── {}", format_duration(d)))
                .unwrap_or_default();

            result.push((
                format!("⏺ {}({}){}", agent_type, description, timing),
                accent,
            ));

            let max_show = 3;
            let total = sub_tools.len();
            for (i, tool_name) in sub_tools.iter().enumerate() {
                if i >= max_show {
                    result.push((
                        format!(
                            "     … +{} more tool uses (ctrl+o to expand)",
                            total - max_show
                        ),
                        dim,
                    ));
                    break;
                }
                let prefix = if i == 0 { "  ⎿  " } else { "     " };
                result.push((format!("{}{}(…)", prefix, capitalize(tool_name)), dim));
            }
            if total == 0 {
                result.push(("  ⎿  (no tool calls)".to_string(), dim));
            }
        }
        ChatRole::SubAgentToolCall { .. } => {}
        ChatRole::SubAgentResult => {}
        ChatRole::AskUser { .. } => {}
    }
    result
}

fn tool_summary_str(name: &str, args: &serde_json::Value) -> String {
    match name {
        "bash" => args["command"].as_str().unwrap_or("???").to_string(),
        "write_file" | "read_file" => args["file_path"].as_str().unwrap_or("???").to_string(),
        "edit_file" => args["file_path"].as_str().unwrap_or("???").to_string(),
        "glob" => args["pattern"].as_str().unwrap_or("???").to_string(),
        "grep" => args["pattern"].as_str().unwrap_or("???").to_string(),
        "agent" => args["description"].as_str().unwrap_or("???").to_string(),
        "memory" => {
            let action = args["action"].as_str().unwrap_or("???");
            let mem_name = args["name"].as_str().unwrap_or("");
            if mem_name.is_empty() {
                action.to_string()
            } else {
                format!("{} {}", action, mem_name)
            }
        }
        "cron" => args["action"].as_str().unwrap_or("???").to_string(),
        "lsp" => {
            let op = args["operation"].as_str().unwrap_or("???");
            let file = args["filePath"].as_str().unwrap_or("");
            if file.is_empty() {
                op.to_string()
            } else {
                format!("{} {}", op, file)
            }
        }
        "enter_worktree" => args["name"].as_str().unwrap_or("").to_string(),
        "notebook_edit" => args["notebook_path"].as_str().unwrap_or("???").to_string(),
        _ => {
            if let Some(obj) = args.as_object() {
                for key in &[
                    "command",
                    "path",
                    "file_path",
                    "relative_path",
                    "query",
                    "pattern",
                    "url",
                    "name",
                ] {
                    if let Some(val) = obj.get(*key).and_then(|v| v.as_str()) {
                        return val.to_string();
                    }
                }
                for val in obj.values() {
                    if let Some(s) = val.as_str() {
                        if s.len() <= 80 {
                            return s.to_string();
                        }
                    }
                }
            }
            String::new()
        }
    }
}
