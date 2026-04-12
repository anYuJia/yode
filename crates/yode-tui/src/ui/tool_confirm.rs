use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use super::palette::{LIGHT, MUTED, PANEL_ACCENT, SELECT_ACCENT};

/// Render inline vertical confirmation selector across 4 viewport lines.
///
/// Layout:
///   Allow Bash(ls)?                     ← line 0: tool name
///   ❯ 1. Yes                            ← line 1
///     2. Yes, always allow [tool]       ← line 2
///     3. No           y/n/a · ↑↓ Enter  ← line 3
pub fn render_inline_confirm(frame: &mut Frame, chunks: &[Rect], app: &App) {
    let Some(ref confirm) = app.pending_confirmation else {
        return;
    };

    let args_display = format_tool_args_short(&confirm.name, &confirm.arguments);

    // Line 0: tool name header
    let header = Line::from(vec![
        Span::styled("  Allow ", Style::default().fg(LIGHT)),
        Span::styled(
            format!("{}({})", capitalize(&confirm.name), args_display),
            Style::default()
                .fg(PANEL_ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("?", Style::default().fg(LIGHT)),
    ]);
    frame.render_widget(Paragraph::new(header), chunks[0]);

    let options = ["Yes", &format!("Yes, always allow {}", confirm.name), "No"];

    // Lines 1-3: options
    for (i, label) in options.iter().enumerate() {
        let is_selected = app.confirm_selected == i;
        let line = if is_selected {
            Line::from(vec![
                Span::styled(
                    "  ❯ ",
                    Style::default()
                        .fg(SELECT_ACCENT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{}. {}", i + 1, label),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ])
        } else {
            Line::from(vec![
                Span::styled("    ", Style::default()),
                Span::styled(format!("{}. {}", i + 1, label), Style::default().fg(MUTED)),
            ])
        };

        // Append hint on last option line
        if i == 2 {
            let mut spans = line.spans;
            spans.push(Span::styled(
                "        y/n/a · ↑↓ Enter",
                Style::default().fg(Color::Indexed(240)),
            ));
            frame.render_widget(Paragraph::new(Line::from(spans)), chunks[i + 1]);
        } else {
            frame.render_widget(Paragraph::new(line), chunks[i + 1]);
        }
    }
}

/// Short summary of tool arguments for the inline header.
fn format_tool_args_short(tool_name: &str, args_json: &str) -> String {
    let parsed: serde_json::Value = match serde_json::from_str(args_json) {
        Ok(v) => v,
        Err(_) => return "…".to_string(),
    };

    match tool_name {
        "bash" => {
            let cmd = parsed["command"].as_str().unwrap_or("???");
            truncate_str(cmd, 60)
        }
        "edit_file" | "write_file" | "read_file" => {
            parsed["file_path"].as_str().unwrap_or("???").to_string()
        }
        _ => {
            if let Some(obj) = parsed.as_object() {
                for key in &["command", "path", "file_path", "query", "pattern"] {
                    if let Some(val) = obj.get(*key).and_then(|v| v.as_str()) {
                        return truncate_str(val, 60);
                    }
                }
            }
            "…".to_string()
        }
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}
