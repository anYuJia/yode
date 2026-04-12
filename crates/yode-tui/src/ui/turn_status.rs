use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, TurnStatus};
use crate::runtime_display::format_retry_delay_summary;

pub fn render_turn_status(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    if area.height == 0 {
        return;
    }

    let indicators = turn_runtime_indicator_text(app);
    let status_line = match &app.turn_status {
        TurnStatus::Idle => return,
        TurnStatus::Working { verb } => {
            let spinner = app.spinner_char();
            let elapsed = app.thinking_elapsed_str();
            let stream_chars = app.streaming_buf.len() as u32;
            let output_tok = app.session.turn_output_tokens + stream_chars / 4;
            Line::from(vec![
                Span::styled(format!("  {} ", spinner), Style::default().fg(Color::LightMagenta)),
                Span::styled(
                    format!("{}…", verb),
                    Style::default().fg(Color::LightMagenta),
                ),
                Span::styled(
                    format!(" ({} · ↓{} tokens)", elapsed, format_tok(output_tok)),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(indicators.clone(), Style::default().fg(Color::DarkGray)),
            ])
        }
        TurnStatus::Done { elapsed, tools } => {
            let elapsed_str = crate::app::format_duration(*elapsed);
            let tools_str = if *tools > 0 {
                format!(" · {} tool calls", tools)
            } else {
                String::new()
            };
            let turn_out = app.session.turn_output_tokens;
            Line::from(vec![Span::styled(
                format!(
                    "  ⚡ Done · {}{} (↓{} tokens){}",
                    elapsed_str,
                    tools_str,
                    format_tok(turn_out),
                    indicators
                ),
                Style::default().fg(Color::DarkGray),
            )])
        }
        TurnStatus::Retrying {
            error,
            attempt,
            max_attempts,
            delay_secs,
        } => Line::from(vec![
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
            Span::styled(indicators, Style::default().fg(Color::DarkGray)),
        ]),
    };

    let lines = if area.height >= 3 {
        vec![Line::from(""), status_line, Line::from("")]
    } else {
        vec![status_line]
    };
    frame.render_widget(Paragraph::new(lines), area);
}

pub fn turn_runtime_indicator_text(app: &App) -> String {
    let Some(engine) = &app.engine else {
        return String::new();
    };
    let Ok(engine) = engine.try_lock() else {
        return String::new();
    };
    let state = engine.runtime_state();
    let mem = if state.live_session_memory_updating {
        format!("mem {}*", state.session_memory_update_count)
    } else if state.live_session_memory_initialized {
        format!("mem {}", state.session_memory_update_count)
    } else {
        "mem cold".to_string()
    };
    let recovery = match state.recovery_state.as_str() {
        "ReanchorRequired" => " · reanchor",
        "SingleStepMode" => " · single-step",
        "NeedUserGuidance" => " · ask-user",
        _ => "",
    };
    format!(" · compact {} · {}{}", state.total_compactions, mem, recovery)
}

fn format_tok(n: u32) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}
