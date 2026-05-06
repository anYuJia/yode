use crate::app::App;
use yode_core::cost_tracker::estimate_token_cost;

pub(super) fn print_exit_summary(app: &App) {
    if let Some(summary) = render_exit_summary(app) {
        eprintln!();
        eprintln!("{summary}");
    }
}

fn render_exit_summary(app: &App) -> Option<String> {
    if app.session.total_tokens == 0 && app.turn_completion.is_empty() {
        return None;
    }

    let elapsed = app.session_start.elapsed();
    let mins = elapsed.as_secs() / 60;
    let secs = elapsed.as_secs() % 60;
    let duration_str = if mins > 0 {
        format!("{}m {:02}s", mins, secs)
    } else {
        format!("{}s", secs)
    };

    let cost = estimate_token_cost(
        &app.session.model,
        app.session.input_tokens.into(),
        app.session.output_tokens.into(),
    );

    let session_short = &app.session.session_id[..app.session.session_id.len().min(8)];
    let mut lines = vec![
        "────────────────────────────────────────".to_string(),
        "Session summary".to_string(),
        format!(
            "  Session:       {} (resume: yode --resume {})",
            session_short, session_short
        ),
        format!("  Duration:      {}", duration_str),
        format!(
            "  Input tokens:  {}",
            format_number(app.session.input_tokens)
        ),
        format!(
            "  Output tokens: {}",
            format_number(app.session.output_tokens)
        ),
        format!(
            "  Total tokens:  {}",
            format_number(app.session.total_tokens)
        ),
        format!("  Tool calls:    {}", app.session.tool_call_count),
        format!("  Est. cost:     ${:.4}", cost),
    ];

    if let Some(turn_message) = app.turn_completion.last_turn_message.as_deref() {
        lines.push(String::new());
        lines.push("Latest turn".to_string());
        lines.extend(turn_message.lines().map(|line| format!("  {}", line)));
    }

    if let Some(memory_message) = app
        .turn_completion
        .last_session_memory_update_message
        .as_deref()
    {
        lines.push(String::new());
        lines.push("Session memory".to_string());
        lines.extend(memory_message.lines().map(|line| format!("  {}", line)));
    }

    lines.push("────────────────────────────────────────".to_string());
    Some(lines.join("\n"))
}

fn format_number(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use yode_llm::registry::ProviderRegistry;
    use yode_tools::registry::ToolRegistry;

    use crate::app::App;

    use super::render_exit_summary;

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
    fn exit_summary_omits_empty_sessions() {
        assert!(render_exit_summary(&test_app()).is_none());
    }

    #[test]
    fn exit_summary_includes_latest_turn_and_memory_sections() {
        let mut app = test_app();
        app.session.input_tokens = 1_200;
        app.session.output_tokens = 180;
        app.session.total_tokens = 1_380;
        app.session.tool_call_count = 3;
        app.turn_completion.last_turn_message = Some(
            "Turn completed · 1.4s · 3 tools · 1.2k↑ 180↓ tok\nsession · 1.4k total tok · 3 tools"
                .to_string(),
        );
        app.turn_completion.last_session_memory_update_message =
            Some("Session memory updated · summary · /tmp/live.md".to_string());

        let summary = render_exit_summary(&app).unwrap();
        assert!(summary.contains("Session summary"));
        assert!(summary.contains("Latest turn"));
        assert!(summary.contains("Turn completed · 1.4s · 3 tools"));
        assert!(summary.contains("Session memory"));
        assert!(summary.contains("Session memory updated · summary · /tmp/live.md"));
    }
}
