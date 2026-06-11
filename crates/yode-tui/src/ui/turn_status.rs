use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, ChatRole, TurnStatus};
use crate::runtime_display::{format_retry_delay_summary, format_tool_progress_summary};
use crate::tool_grouping::{
    describe_tool_call, detect_groupable_tool_batch, summarize_groupable_tool_call,
    tool_batch_hint_text, tool_batch_summary_text,
};
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
        TurnStatus::Idle => {
            let Some(last_turn) = app.turn_completion.last_turn_message.as_deref() else {
                return;
            };
            Line::from(vec![
                Span::styled("  ⚡ ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    compact_last_turn_status(last_turn),
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        }
        TurnStatus::Working { verb } => {
            let spinner = app.spinner_char();
            let elapsed = app.thinking_elapsed_str();
            let stream_chars = app.streaming_buf.len() as u32;
            let output_tok = app.session.turn_output_tokens + stream_chars / 4;
            let working_label = active_working_label(app, verb);
            let mut spans = vec![
                Span::styled(
                    format!("  {} ", spinner),
                    Style::default().fg(Color::LightMagenta),
                ),
                Span::styled(working_label, Style::default().fg(Color::LightMagenta)),
                Span::styled(
                    format!(
                        " ({} · {})",
                        elapsed,
                        compact_output_token_phrase(output_tok)
                    ),
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
                    "  ⚡ 已完成 · {} ({})",
                    elapsed_str,
                    compact_output_token_phrase(turn_out)
                ),
                Style::default().fg(Color::DarkGray),
            )];
            if *tools > 0 {
                spans.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
                spans.push(Span::styled(
                    match density {
                        crate::ui::responsive::Density::Wide => format!("{} 个工具", tools),
                        crate::ui::responsive::Density::Medium
                        | crate::ui::responsive::Density::Narrow => format!("t{}", tools),
                    },
                    Style::default().fg(Color::DarkGray),
                ));
            }
            if !indicator_spans.is_empty() {
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
    let working_hint = matches!(app.turn_status, TurnStatus::Working { .. })
        .then(|| active_working_hint(app))
        .flatten();

    let mut lines = Vec::new();
    if area.height >= 3 {
        lines.push(Line::from(""));
    }
    lines.push(status_line);
    if let Some(hint) = working_hint {
        lines.push(Line::from(vec![
            Span::styled("  ⎿ ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                hint,
                Style::default()
                    .fg(Color::Gray)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]));
    } else if area.height >= 3 {
        lines.push(Line::from(""));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

pub(crate) fn active_working_label(app: &App, fallback_verb: &str) -> String {
    for index in (0..app.chat_entries.len()).rev() {
        if let Some(batch) = detect_groupable_tool_batch(&app.chat_entries, index) {
            if batch.is_active && batch.next_index == app.chat_entries.len() {
                return tool_batch_summary_text(&batch);
            }
        }
    }

    if let Some(entry) = app.chat_entries.last() {
        if let ChatRole::ToolCall { id, name } = &entry.role {
            let has_result = app.chat_entries.iter().rev().skip(1).any(|candidate| {
                matches!(&candidate.role, ChatRole::ToolResult { id: result_id, .. } if result_id == id)
            });
            if !has_result {
                if let Some(summary) = summarize_groupable_tool_call(name, &entry.content, true) {
                    return summary;
                }
                let args: serde_json::Value =
                    serde_json::from_str(&entry.content).unwrap_or(serde_json::Value::Null);
                if let Some(summary) = describe_tool_call(name, &args, true) {
                    return ensure_active_ellipsis(&summary);
                }
                if let Some(summary) = tool_activity_label(app, name, &entry.content) {
                    return summary;
                }
            }
        }
    }

    format!("{}…", fallback_verb)
}

pub(crate) fn active_working_hint(app: &App) -> Option<String> {
    if let Some(state) = app
        .engine
        .as_ref()
        .and_then(|engine| engine.try_lock().ok())
        .map(|engine| engine.runtime_state())
    {
        let progress = format_tool_progress_summary(
            state.last_tool_progress_tool.as_deref(),
            state.last_tool_progress_message.as_deref(),
            state.last_tool_progress_at.as_deref(),
        );
        if progress != "none" {
            return Some(progress);
        }
    }

    for index in (0..app.chat_entries.len()).rev() {
        if let Some(batch) = detect_groupable_tool_batch(&app.chat_entries, index) {
            if batch.is_active && batch.next_index == app.chat_entries.len() {
                return tool_batch_hint_text(&app.chat_entries, &batch);
            }
        }
    }

    if let Some(entry) = app.chat_entries.last() {
        if let ChatRole::ToolCall { id, name } = &entry.role {
            let has_result = app.chat_entries.iter().rev().skip(1).any(|candidate| {
                matches!(&candidate.role, ChatRole::ToolResult { id: result_id, .. } if result_id == id)
            });
            if !has_result {
                let args: serde_json::Value =
                    serde_json::from_str(&entry.content).unwrap_or(serde_json::Value::Null);
                return describe_tool_call(name, &args, true);
            }
        }
    }

    None
}

fn tool_activity_label(app: &App, tool_name: &str, args_json: &str) -> Option<String> {
    let tool = app.tools.get(tool_name)?;
    let args: serde_json::Value =
        serde_json::from_str(args_json).unwrap_or(serde_json::Value::Null);
    let description = tool.activity_description(&args);
    if description.trim().is_empty() {
        return None;
    }
    Some(ensure_active_ellipsis(&description))
}

fn ensure_active_ellipsis(text: &str) -> String {
    let trimmed = text.trim_end();
    if trimmed.ends_with('…') || trimmed.ends_with("...") {
        trimmed.to_string()
    } else if let Some(stripped) = trimmed.strip_suffix('.') {
        format!("{}…", stripped)
    } else {
        format!("{}…", trimmed)
    }
}

fn format_tok(n: u32) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

fn compact_output_token_phrase(n: u32) -> String {
    format!("↓{} tok", format_tok(n))
}

fn compact_last_turn_status(message: &str) -> String {
    message
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("回合已完成")
        .trim()
        .replacen("Turn completed", "已完成", 1)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use yode_llm::registry::ProviderRegistry;
    use yode_tools::builtin::EditFileTool;
    use yode_tools::registry::ToolRegistry;

    use crate::app::{App, ChatEntry, ChatRole};

    use super::{
        active_working_hint, active_working_label, compact_last_turn_status,
        compact_output_token_phrase, format_tok,
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
    fn working_label_prefers_active_tool_batch_summary() {
        let mut app = test_app();
        app.chat_entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "web_search".to_string(),
                },
                "{\"query\":\"ratatui\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "b".to_string(),
                    name: "read_file".to_string(),
                },
                "{\"file_path\":\"/tmp/src/main.rs\"}".to_string(),
            ),
        ];

        assert_eq!(
            active_working_label(&app, "Working"),
            "正在进行 1次联网搜索，正在读取 1个文件..."
        );
    }

    #[test]
    fn working_label_falls_back_to_turn_verb_without_active_batch() {
        let app = test_app();
        assert_eq!(active_working_label(&app, "Analyzing"), "Analyzing…");
    }

    #[test]
    fn working_and_done_status_use_compact_tok_suffix() {
        assert_eq!(format_tok(1200), "1.2k");
        assert_eq!(compact_output_token_phrase(1200), "↓1.2k tok");
    }

    #[test]
    fn compact_last_turn_status_keeps_time_tools_and_tokens() {
        let status = compact_last_turn_status(
            "Turn completed · 2.0s · 3 tools · 10↑ 20↓ tok\nsession · 30 total tok · 3 tools",
        );

        assert_eq!(status, "已完成 · 2.0s · 3 tools · 10↑ 20↓ tok");
    }

    #[test]
    fn working_label_uses_single_tool_summary_when_only_one_tool_is_active() {
        let mut app = test_app();
        app.chat_entries = vec![ChatEntry::new(
            ChatRole::ToolCall {
                id: "a".to_string(),
                name: "project_map".to_string(),
            },
            "{}".to_string(),
        )];

        assert_eq!(active_working_label(&app, "Working"), "正在分析 1个项目...");
    }

    #[test]
    fn working_label_prefers_compact_tool_call_description_for_non_groupable_tools() {
        let registry = Arc::new(ToolRegistry::new());
        registry.register(Arc::new(EditFileTool));
        let mut app = App::new(
            "test-model".to_string(),
            "session-1234".to_string(),
            "/tmp".to_string(),
            "test".to_string(),
            Vec::new(),
            HashMap::new(),
            Arc::new(ProviderRegistry::new()),
            registry,
        );
        app.chat_entries = vec![ChatEntry::new(
            ChatRole::ToolCall {
                id: "a".to_string(),
                name: "edit_file".to_string(),
            },
            "{\"file_path\":\"/tmp/demo.rs\",\"old_string\":\"a\",\"new_string\":\"b\"}"
                .to_string(),
        )];

        assert_eq!(
            active_working_label(&app, "Working"),
            "正在编辑 .../tmp/demo.rs…"
        );
    }

    #[test]
    fn working_hint_prefers_latest_grouped_target() {
        let mut app = test_app();
        app.chat_entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "grep".to_string(),
                },
                "{\"pattern\":\"retry\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "a".to_string(),
                    name: "grep".to_string(),
                    is_error: false,
                },
                "ok".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "b".to_string(),
                    name: "read_file".to_string(),
                },
                "{\"file_path\":\"/tmp/src/main.rs\"}".to_string(),
            ),
        ];

        assert_eq!(
            active_working_hint(&app).as_deref(),
            Some("正在读取 .../src/main.rs")
        );
    }
}
