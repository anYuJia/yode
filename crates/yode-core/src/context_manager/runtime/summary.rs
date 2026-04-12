use std::collections::BTreeMap;

use yode_llm::types::{Message, Role};

use super::{is_context_summary, CONTEXT_SUMMARY_PREFIX, SUMMARY_CHAR_BUDGET};

pub(crate) fn build_context_summary(
    removed_messages: &[Message],
    tool_results_truncated: usize,
    turn_artifact_path: Option<&str>,
) -> Option<String> {
    let mut user_goals = Vec::new();
    let mut assistant_findings = Vec::new();
    let mut tool_usage: BTreeMap<String, usize> = BTreeMap::new();
    let mut removed_tool_results = 0usize;

    for msg in removed_messages {
        if is_context_summary(msg) {
            continue;
        }

        match msg.role {
            Role::User => {
                if user_goals.len() < 3 {
                    if let Some(content) = msg.content.as_deref() {
                        if let Some(excerpt) = excerpt(content, 120) {
                            if !user_goals.contains(&excerpt) {
                                user_goals.push(excerpt);
                            }
                        }
                    }
                }
            }
            Role::Assistant => {
                for tool_call in &msg.tool_calls {
                    *tool_usage.entry(tool_call.name.clone()).or_insert(0) += 1;
                }
                if msg.tool_calls.is_empty() && assistant_findings.len() < 3 {
                    if let Some(content) = msg.content.as_deref() {
                        if let Some(excerpt) = excerpt(content, 140) {
                            if !assistant_findings.contains(&excerpt) {
                                assistant_findings.push(excerpt);
                            }
                        }
                    }
                }
            }
            Role::Tool => {
                removed_tool_results += 1;
            }
            Role::System => {}
        }
    }

    if user_goals.is_empty()
        && assistant_findings.is_empty()
        && tool_usage.is_empty()
        && removed_tool_results == 0
        && tool_results_truncated == 0
    {
        return None;
    }

    let lines = context_summary_lines(
        removed_messages.len(),
        &user_goals,
        &assistant_findings,
        &tool_usage,
        removed_tool_results,
        tool_results_truncated,
        turn_artifact_path,
    );

    let mut summary = lines.join("\n");
    if summary.chars().count() > SUMMARY_CHAR_BUDGET {
        summary = summary
            .chars()
            .take(SUMMARY_CHAR_BUDGET)
            .collect::<String>();
        summary.push_str("...");
    }

    Some(summary)
}

pub(crate) fn context_summary_lines(
    removed_count: usize,
    user_goals: &[String],
    assistant_findings: &[String],
    tool_usage: &BTreeMap<String, usize>,
    removed_tool_results: usize,
    tool_results_truncated: usize,
    turn_artifact_path: Option<&str>,
) -> Vec<String> {
    let mut lines = vec![
        format!(
            "{} Older conversation was compacted to stay within the model window.",
            CONTEXT_SUMMARY_PREFIX
        ),
        format!("- Removed messages: {}", removed_count),
    ];

    if let Some(path) = turn_artifact_path.filter(|path| !path.trim().is_empty()) {
        lines.push(format!("- Turn artifact: {}", path));
    }

    if !user_goals.is_empty() {
        lines.push(format!("- Earlier user goals: {}", user_goals.join(" | ")));
    }
    if !assistant_findings.is_empty() {
        lines.push(format!(
            "- Earlier assistant findings: {}",
            assistant_findings.join(" | ")
        ));
    }
    if !tool_usage.is_empty() {
        let tool_summary = tool_usage
            .iter()
            .take(5)
            .map(|(name, count)| {
                if *count > 1 {
                    format!("{} x{}", name, count)
                } else {
                    name.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("- Earlier tool activity: {}", tool_summary));
    }

    if removed_tool_results > 0 || tool_results_truncated > 0 {
        lines.push(format!(
            "- Tool results compacted: {} removed, {} truncated",
            removed_tool_results, tool_results_truncated
        ));
    }

    lines
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

pub(crate) fn calibration_token_estimate(
    char_count: usize,
    last_known_prompt_tokens: Option<u32>,
    last_known_char_count: Option<usize>,
) -> usize {
    if let Some(known_tokens) = last_known_prompt_tokens {
        if let Some(known_chars) = last_known_char_count {
            if known_chars > 0 {
                return ((char_count as f64) * (known_tokens as f64 / known_chars as f64)) as usize;
            }
        }
    }
    char_count / 4
}

pub(crate) fn messages_char_count(messages: &[Message]) -> usize {
    messages.iter().map(Message::estimated_char_count).sum()
}
