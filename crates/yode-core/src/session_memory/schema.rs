use super::*;

pub(in crate::session_memory) fn render_structured_sections(
    lines: &mut Vec<String>,
    sections: &StructuredMemorySections,
    files_read_summary: Option<&str>,
    files_modified_summary: Option<&str>,
    hints: &MemorySchemaHints,
) {
    push_bullet_section(lines, "Goals", &sections.goals);
    push_bullet_section(lines, "Findings", &sections.findings);
    push_bullet_section(lines, "Decisions", &sections.decisions);

    lines.push("### Files".to_string());
    lines.push(String::new());
    let mut wrote_file_line = false;
    if let Some(read_summary) = files_read_summary {
        lines.push(format!("- Read: {}", read_summary));
        wrote_file_line = true;
    }
    if let Some(modified_summary) = files_modified_summary {
        lines.push(format!("- Modified: {}", modified_summary));
        wrote_file_line = true;
    }
    if !wrote_file_line {
        lines.push("- None".to_string());
    }
    lines.push(String::new());

    push_bullet_section(lines, "Open Questions", &sections.open_questions);
    push_bullet_section(lines, "Freshness", &hints.freshness);
    push_bullet_section(lines, "Confidence", &hints.confidence);
}

fn push_bullet_section(lines: &mut Vec<String>, title: &str, items: &[String]) {
    lines.push(format!("### {}", title));
    lines.push(String::new());
    if items.is_empty() {
        lines.push("- None".to_string());
    } else {
        for item in items {
            lines.push(format!("- {}", item));
        }
    }
    lines.push(String::new());
}

pub(in crate::session_memory) fn structured_sections_from_compaction_summary(
    summary: Option<&str>,
) -> StructuredMemorySections {
    let Some(summary) = summary else {
        return StructuredMemorySections {
            findings: vec![
                "Tool-result trimming reclaimed space without generating a summary anchor."
                    .to_string(),
            ],
            ..Default::default()
        };
    };

    let mut sections = StructuredMemorySections::default();
    for line in summary.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(value) = trimmed.strip_prefix("[Context summary]") {
            let value = value.trim();
            if !value.is_empty() {
                sections.findings.push(value.to_string());
            }
        } else if let Some(value) = trimmed.strip_prefix("- Earlier user goals: ") {
            sections.goals.extend(split_pipe_items(value));
        } else if let Some(value) = trimmed.strip_prefix("- Earlier assistant findings: ") {
            sections.findings.extend(split_pipe_items(value));
        } else if let Some(value) = trimmed.strip_prefix("- Earlier tool activity: ") {
            sections
                .findings
                .push(format!("Tool activity: {}", value.trim()));
        } else if let Some(value) = trimmed.strip_prefix("- Turn artifact: ") {
            sections
                .findings
                .push(format!("Turn artifact: {}", value.trim()));
        } else if let Some(value) = trimmed.strip_prefix("- Tool results compacted: ") {
            sections
                .findings
                .push(format!("Tool results compacted: {}", value.trim()));
        }
    }

    if sections.goals.is_empty() && sections.findings.is_empty() {
        sections.findings.push(summary.trim().to_string());
    }

    dedupe_section_items(&mut sections.goals);
    dedupe_section_items(&mut sections.findings);
    dedupe_section_items(&mut sections.decisions);
    dedupe_section_items(&mut sections.open_questions);
    sections
}

pub(in crate::session_memory) fn live_memory_hints(generated_at: &str) -> MemorySchemaHints {
    MemorySchemaHints {
        freshness: vec![
            format!("Generated at: {}", generated_at),
            "Current-session snapshot; prefer this over older compacted entries.".to_string(),
        ],
        confidence: vec![
            "High for goals/files; medium for inferred findings and decisions.".to_string(),
            "Derived from direct recent session messages and file activity.".to_string(),
        ],
    }
}

pub(in crate::session_memory) fn compaction_memory_hints(generated_at: &str) -> MemorySchemaHints {
    MemorySchemaHints {
        freshness: vec![
            format!("Generated at: {}", generated_at),
            "Point-in-time compact snapshot; verify against current code if the session has moved on."
                .to_string(),
        ],
        confidence: vec![
            "Medium; synthesized from compaction summary plus current-turn file activity."
                .to_string(),
            "Use transcript artifacts when a removed detail needs exact recovery.".to_string(),
        ],
    }
}

pub(in crate::session_memory) fn normalize_live_summary_markdown(
    summary: &str,
    snapshot: &LiveSessionSnapshot,
    hints: &MemorySchemaHints,
) -> String {
    let trimmed = summary.trim();
    if trimmed.contains("### Goals") || trimmed.contains("### Findings") {
        let mut output = trimmed.to_string();
        if !trimmed.contains("### Freshness") {
            if !output.ends_with('\n') {
                output.push('\n');
            }
            output.push('\n');
            output.push_str(&render_named_section("Freshness", &hints.freshness));
        }
        if !trimmed.contains("### Confidence") {
            if !output.ends_with('\n') {
                output.push('\n');
            }
            output.push('\n');
            output.push_str(&render_named_section("Confidence", &hints.confidence));
        }
        return output;
    }

    let sections = StructuredMemorySections {
        goals: snapshot.goals.clone(),
        findings: vec![trimmed.to_string()],
        decisions: snapshot.decisions.clone(),
        open_questions: snapshot.open_questions.clone(),
    };
    let files_read_summary = super::io::summarize_entries(snapshot.files_read.clone());
    let files_modified_summary = super::io::summarize_entries(snapshot.files_modified.clone());
    let mut lines = Vec::new();
    render_structured_sections(
        &mut lines,
        &sections,
        files_read_summary.as_deref(),
        files_modified_summary.as_deref(),
        hints,
    );
    lines.join("\n")
}

fn render_named_section(title: &str, items: &[String]) -> String {
    let mut lines = vec![format!("### {}", title), String::new()];
    if items.is_empty() {
        lines.push("- None".to_string());
    } else {
        for item in items {
            lines.push(format!("- {}", item));
        }
    }
    lines.join("\n")
}

fn split_pipe_items(value: &str) -> Vec<String> {
    value
        .split('|')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn dedupe_section_items(items: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    items.retain(|item| seen.insert(item.clone()));
}
