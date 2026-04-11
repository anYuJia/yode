use super::*;

pub fn build_live_snapshot(
    session_id: &str,
    messages: &[Message],
    total_tool_calls: u32,
    files_read: &[String],
    files_modified: &[String],
) -> LiveSessionSnapshot {
    let mut user_goals = Vec::new();
    let mut assistant_findings = Vec::new();
    let mut decisions = Vec::new();
    let mut open_questions = Vec::new();

    for message in messages.iter().rev() {
        match message.role {
            Role::User => {
                if let Some(content) = message.content.as_deref() {
                    push_unique_excerpt(&mut user_goals, content, 160, 3);
                    if looks_like_open_question(content) {
                        push_unique_excerpt(&mut open_questions, content, 180, 3);
                    }
                }
            }
            Role::Assistant if message.tool_calls.is_empty() => {
                if let Some(content) = message.content.as_deref() {
                    push_unique_excerpt(&mut assistant_findings, content, 180, 3);
                    if looks_like_decision(content) {
                        push_unique_excerpt(&mut decisions, content, 180, 3);
                    }
                    if looks_like_open_question(content) {
                        push_unique_excerpt(&mut open_questions, content, 180, 3);
                    }
                }
            }
            _ => {}
        }
    }

    LiveSessionSnapshot {
        session_id: session_id.to_string(),
        total_tool_calls,
        message_count: messages.len(),
        goals: user_goals,
        findings: assistant_findings,
        decisions,
        open_questions,
        files_read: dedupe_entries(files_read),
        files_modified: dedupe_entries(files_modified),
    }
}

pub fn render_live_session_memory_prompt(
    existing_summary: Option<&str>,
    snapshot: &LiveSessionSnapshot,
    recent_messages: &[Message],
) -> String {
    let mut prompt = String::new();
    prompt.push_str(
        "Update the session memory for an AI coding assistant.\n\
         Produce concise markdown with these sections in order:\n\
         1. Goals\n2. Findings\n3. Decisions\n4. Files\n5. Open Questions\n\n\
         Rules:\n\
         - Keep only verified facts and the current active direction.\n\
         - Prefer concrete file paths and technical constraints.\n\
         - Use `- None` for empty sections.\n\
         - Omit chatter, duplicated history, and completed low-value details.\n\
         - Keep the whole output under 260 words.\n\
         - Return markdown only.\n\n",
    );

    if let Some(existing) = existing_summary {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            prompt.push_str("Existing session memory:\n```md\n");
            prompt.push_str(trimmed);
            prompt.push_str("\n```\n\n");
        }
    }

    prompt.push_str("Deterministic snapshot:\n");
    prompt.push_str(&render_live_snapshot(snapshot));
    prompt.push_str("\n\nRecent messages:\n");
    prompt.push_str(&format_recent_messages(recent_messages));
    prompt
}

pub(in crate::session_memory) fn render_live_snapshot(snapshot: &LiveSessionSnapshot) -> String {
    let generated_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut lines = vec![
        format!(
            "## {} session {}",
            generated_at,
            snapshot.session_id.chars().take(8).collect::<String>()
        ),
        String::new(),
        format!(
            "- Total tool calls this session: {}",
            snapshot.total_tool_calls
        ),
        format!("- Current message count: {}", snapshot.message_count),
        String::new(),
    ];
    let sections = StructuredMemorySections {
        goals: snapshot.goals.clone(),
        findings: snapshot.findings.clone(),
        decisions: snapshot.decisions.clone(),
        open_questions: snapshot.open_questions.clone(),
    };
    let files_read_summary = super::io::summarize_entries(snapshot.files_read.clone());
    let files_modified_summary = super::io::summarize_entries(snapshot.files_modified.clone());
    let hints = super::schema::live_memory_hints(&generated_at);
    super::schema::render_structured_sections(
        &mut lines,
        &sections,
        files_read_summary.as_deref(),
        files_modified_summary.as_deref(),
        &hints,
    );

    lines.join("\n")
}

fn push_unique_excerpt(target: &mut Vec<String>, content: &str, limit: usize, max_items: usize) {
    if target.len() >= max_items {
        return;
    }
    if let Some(excerpt) = excerpt(content, limit) {
        if !target.contains(&excerpt) {
            target.push(excerpt);
        }
    }
}

fn looks_like_decision(content: &str) -> bool {
    let normalized = content.trim().to_lowercase();
    normalized.starts_with("i will ")
        || normalized.starts_with("we will ")
        || normalized.starts_with("we'll ")
        || normalized.starts_with("use ")
        || normalized.starts_with("keep ")
        || normalized.starts_with("switch ")
        || normalized.starts_with("prefer ")
        || normalized.contains(" decided ")
        || normalized.contains(" decision ")
        || normalized.contains(" plan is ")
}

fn looks_like_open_question(content: &str) -> bool {
    let normalized = content.to_lowercase();
    content.contains('?')
        || normalized.contains("not sure")
        || normalized.contains("unknown")
        || normalized.contains("unclear")
        || normalized.contains("need to verify")
        || normalized.contains("follow up")
}

fn format_recent_messages(messages: &[Message]) -> String {
    let mut lines = Vec::new();
    for message in messages {
        let role = match message.role {
            Role::System => "System",
            Role::User => "User",
            Role::Assistant => "Assistant",
            Role::Tool => "Tool",
        };

        if let Some(content) = message.content.as_deref() {
            if let Some(excerpt) = excerpt(content, 220) {
                lines.push(format!("{}: {}", role, excerpt));
            }
        }
    }

    lines.join("\n")
}

fn excerpt(text: &str, limit: usize) -> Option<String> {
    let squashed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if squashed.is_empty() {
        return None;
    }

    let shortened: String = squashed.chars().take(limit).collect();
    if squashed.chars().count() > limit {
        Some(format!("{}...", shortened.trim_end()))
    } else {
        Some(shortened)
    }
}

fn dedupe_entries(entries: &[String]) -> Vec<String> {
    entries
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
