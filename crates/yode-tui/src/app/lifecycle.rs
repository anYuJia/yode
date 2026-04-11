use crate::app::App;

pub(super) fn print_exit_summary(app: &App) {
    if app.session.total_tokens == 0 {
        return;
    }
    let elapsed = app.session_start.elapsed();
    let mins = elapsed.as_secs() / 60;
    let secs = elapsed.as_secs() % 60;
    let duration_str = if mins > 0 {
        format!("{}m {:02}s", mins, secs)
    } else {
        format!("{}s", secs)
    };

    let cost = super::commands::estimate_cost(
        &app.session.model,
        app.session.input_tokens,
        app.session.output_tokens,
    );

    let session_short = &app.session.session_id[..app.session.session_id.len().min(8)];

    eprintln!();
    eprintln!("────────────────────────────────────────");
    eprintln!("Session summary");
    eprintln!(
        "  Session:       {} (resume: yode --resume {})",
        session_short, session_short
    );
    eprintln!("  Duration:      {}", duration_str);
    eprintln!(
        "  Input tokens:  {}",
        format_number(app.session.input_tokens)
    );
    eprintln!(
        "  Output tokens: {}",
        format_number(app.session.output_tokens)
    );
    eprintln!(
        "  Total tokens:  {}",
        format_number(app.session.total_tokens)
    );
    eprintln!("  Tool calls:    {}", app.session.tool_call_count);
    eprintln!("  Est. cost:     ${:.4}", cost);
    eprintln!("────────────────────────────────────────");
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
