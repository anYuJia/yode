use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, TurnStatus};
use crate::runtime_display::format_retry_delay_summary;
use crate::ui::responsive::density_from_width;
use crate::ui::status_summary::{
    compaction_badge, context_badge, memory_badge, push_badge, runtime_status_snapshot,
    tool_progress_badge, turn_tool_badge,
};

pub fn render_turn_status(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    if area.height == 0 {
        return;
    }

    let density = density_from_width(area.width, 72, 108);
    let snapshot = runtime_status_snapshot(app);
    let fallback_context_tokens: usize = app
        .chat_entries
        .iter()
        .map(|e| e.content.len())
        .sum::<usize>()
        / 4;
    let mut indicator_spans: Vec<Span<'static>> = Vec::new();
    if let Some(badge) = tool_progress_badge(snapshot.state.as_ref(), density) {
        push_badge(&mut indicator_spans, badge);
    }
    if let Some(badge) = turn_tool_badge(snapshot.state.as_ref(), app.turn_tool_count, density) {
        push_badge(&mut indicator_spans, badge);
    }
    push_badge(
        &mut indicator_spans,
        context_badge(snapshot.state.as_ref(), fallback_context_tokens, density),
    );
    if let Some(badge) = compaction_badge(snapshot.state.as_ref(), density) {
        push_badge(&mut indicator_spans, badge);
    }
    if let Some(badge) = memory_badge(snapshot.state.as_ref(), density) {
        push_badge(&mut indicator_spans, badge);
    }
    if matches!(indicator_spans.last(), Some(span) if span.content == "· ") {
        indicator_spans.pop();
    }
    let status_line = match &app.turn_status {
        TurnStatus::Idle => return,
        TurnStatus::Working { verb } => {
            let spinner = app.spinner_char();
            let elapsed = app.thinking_elapsed_str();
            let stream_chars = app.streaming_buf.len() as u32;
            let output_tok = app.session.turn_output_tokens + stream_chars / 4;
            let mut spans = vec![
                Span::styled(
                    format!("  {} ", spinner),
                    Style::default().fg(Color::LightMagenta),
                ),
                Span::styled(
                    format!("{}…", verb),
                    Style::default().fg(Color::LightMagenta),
                ),
                Span::styled(
                    format!(" ({} · ↓{} tokens)", elapsed, format_tok(output_tok)),
                    Style::default().fg(Color::DarkGray),
                ),
            ];
            if !indicator_spans.is_empty() {
                spans.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
                spans.extend(indicator_spans.clone());
            }
            Line::from(spans)
        }
        TurnStatus::Done { elapsed, tools } => {
            let elapsed_str = crate::app::format_duration(*elapsed);
            let turn_out = app.session.turn_output_tokens;
            let mut spans = vec![Span::styled(
                format!(
                    "  ⚡ Done · {} (↓{} tokens)",
                    elapsed_str,
                    format_tok(turn_out)
                ),
                Style::default().fg(Color::DarkGray),
            )];
            if *tools > 0 {
                spans.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
                spans.push(Span::styled(
                    match density {
                        crate::ui::responsive::Density::Wide => format!("{} tools", tools),
                        crate::ui::responsive::Density::Medium
                        | crate::ui::responsive::Density::Narrow => format!("t{}", tools),
                    },
                    Style::default().fg(Color::DarkGray),
                ));
            }
            if *tools > 0 && !indicator_spans.is_empty() {
                spans.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
            } else if *tools == 0 && !indicator_spans.is_empty() {
                spans.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
            }
            spans.extend(indicator_spans.clone());
            Line::from(spans)
        }
        TurnStatus::Retrying {
            verb: _,
            error,
            attempt,
            max_attempts,
            delay_secs,
        } => {
            let mut spans = vec![
                Span::styled(
                    format!("  ⎿ {}", error),
                    Style::default().fg(Color::LightRed),
                ),
                Span::styled(
                    format!(
                        " · {}",
                        format_retry_delay_summary(*delay_secs, *attempt, *max_attempts)
                    ),
                    Style::default().fg(Color::Yellow),
                ),
            ];
            if !indicator_spans.is_empty() {
                spans.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
                spans.extend(indicator_spans);
            }
            Line::from(spans)
        }
    };

    let lines = if area.height >= 3 {
        vec![Line::from(""), status_line, Line::from("")]
    } else {
        vec![status_line]
    };
    frame.render_widget(Paragraph::new(lines), area);
}

fn format_tok(n: u32) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}
