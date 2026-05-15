use super::*;
use yode_llm::types::RestoreSystemBlockHint;

const SESSION_MEMORY_SUMMARY_PREFIX: &str = "[Context summary]";
const SESSION_MEMORY_SUMMARY_MAX_CHARS: usize = 1_200;
const LLM_COMPACTION_SUMMARY_MAX_CHARS: usize = 3_200;
const LLM_COMPACTION_TRANSCRIPT_CHAR_BUDGET: usize = 28_000;
const LLM_COMPACTION_MAX_RETRIES: usize = 3;
const POST_COMPACT_RUNTIME_PREFIX: &str = "[Post-compact restore: runtime]";
const POST_COMPACT_FILES_PREFIX: &str = "[Post-compact restore: files]";
const POST_COMPACT_PLAN_PREFIX: &str = "[Post-compact restore: plan]";
const POST_COMPACT_TOOLS_PREFIX: &str = "[Post-compact restore: tools]";
const POST_COMPACT_PROMPT_CACHE_PREFIX: &str = "[Post-compact restore: prompt-cache]";
const POST_COMPACT_SKILLS_PREFIX: &str = "[Post-compact restore: skills]";
const POST_COMPACT_MCP_PREFIX: &str = "[Post-compact restore: mcp]";
const POST_COMPACT_ARTIFACTS_PREFIX: &str = "[Post-compact restore: artifacts]";
const HIDDEN_POST_COMPACT_RESTORE_PREFIX: &str = "# Post-compact Restore";
const REACTIVE_GAP_SAFETY_TOKENS: usize = 2_000;
const POST_COMPACT_FILE_EXCERPT_MAX_FILES: usize = 3;
const POST_COMPACT_FILE_EXCERPT_MAX_CHARS: usize = 900;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompactionMode {
    Auto,
    Manual,
    Reactive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompactionSummaryScope {
    Full,
    PartialUpTo,
    PartialFrom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum RestoreBlockKind {
    Runtime,
    Files,
    Plan,
    Tools,
    PromptCache,
    Skills,
    Mcp,
    Artifacts,
}

impl RestoreBlockKind {
    fn label(self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::Files => "files",
            Self::Plan => "plan",
            Self::Tools => "tools",
            Self::PromptCache => "prompt-cache",
            Self::Skills => "skills",
            Self::Mcp => "mcp",
            Self::Artifacts => "artifacts",
        }
    }
}

#[derive(serde::Serialize)]
struct RestoreBlockArtifact<'a> {
    kind: &'a str,
    content: &'a str,
    fingerprint: String,
}

#[derive(serde::Deserialize)]
struct OwnedRestoreBlockArtifact {
    kind: String,
    content: String,
    #[serde(rename = "fingerprint")]
    _fingerprint: String,
}

impl CompactionMode {
    fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
            Self::Reactive => "reactive",
        }
    }

    fn is_auto(self) -> bool {
        matches!(self, Self::Auto)
    }
}

impl CompactionSummaryScope {
    fn prompt_guidance(self) -> &'static str {
        match self {
            Self::Full => {
                "Scope: full compact. Summarize the compacted conversation so the next turn can continue from the kept recent tail."
            }
            Self::PartialUpTo => {
                "Scope: partial compact up_to. You are summarizing the older prefix before a selected point. Preserve durable goals, decisions, files, constraints, and handoff state; do not imply the newer tail is included because it remains verbatim after this summary."
            }
            Self::PartialFrom => {
                "Scope: partial compact from. You are summarizing the later tail after a selected point. Earlier messages remain verbatim before this summary, so focus on actionable work, findings, and next steps from the summarized tail."
            }
        }
    }
}

fn is_prompt_too_long_text(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    normalized.contains("prompt too long")
        || normalized.contains("context window")
        || normalized.contains("context length")
        || normalized.contains("maximum context")
        || normalized.contains("too many tokens")
        || normalized.contains("input is too long")
        || normalized.contains("input tokens")
}

fn is_media_size_error_text(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    (normalized.contains("image exceeds") && normalized.contains("maximum"))
        || normalized.contains("image dimensions exceed")
        || normalized.contains("too many images")
        || normalized.contains("maximum of") && normalized.contains("pdf pages")
        || normalized.contains("image was too large")
        || normalized.contains("request too large")
        || normalized.contains("file was too large")
}

fn parse_prompt_too_long_token_gap(text: &str) -> Option<usize> {
    let regex =
        Regex::new(r"prompt(?:\s+is)?\s+too\s+long[^0-9]*(\d+)\s*tokens?\s*>\s*(\d+)").ok()?;
    let captures = regex.captures(text)?;
    let actual = captures.get(1)?.as_str().parse::<usize>().ok()?;
    let limit = captures.get(2)?.as_str().parse::<usize>().ok()?;
    actual.checked_sub(limit).filter(|gap| *gap > 0)
}

fn display_compaction_memory_path(
    project_root: &std::path::Path,
    path: &std::path::Path,
) -> String {
    path.strip_prefix(project_root)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

fn build_session_memory_compaction_summary(
    project_root: &std::path::Path,
    path: &std::path::Path,
    excerpt: &str,
) -> String {
    let mut summary = format!(
        "{} Earlier conversation was compacted using persisted session memory.\n- Session memory source: {}\n- Carry-over:\n{}",
        SESSION_MEMORY_SUMMARY_PREFIX,
        display_compaction_memory_path(project_root, path),
        excerpt
    );
    if summary.chars().count() > SESSION_MEMORY_SUMMARY_MAX_CHARS {
        summary = summary
            .chars()
            .take(SESSION_MEMORY_SUMMARY_MAX_CHARS)
            .collect::<String>();
        summary.push_str("...");
    }
    summary
}

fn summarize_string_entries(entries: &[String], max_items: usize) -> Option<String> {
    if entries.is_empty() {
        return None;
    }

    let mut values = entries.to_vec();
    values.sort();
    values.dedup();
    let extra = values.len().saturating_sub(max_items);
    values.truncate(max_items);
    let mut summary = values.join(", ");
    if extra > 0 {
        summary.push_str(&format!(", +{} more", extra));
    }
    Some(summary)
}

fn resolve_post_compact_file_path(
    project_root: &std::path::Path,
    cwd: &str,
    file_path: &str,
) -> Option<std::path::PathBuf> {
    let file_path = file_path.trim();
    if file_path.is_empty() {
        return None;
    }

    let canonical_root = std::fs::canonicalize(project_root).ok()?;
    let raw_path = std::path::Path::new(file_path);
    let candidates = if raw_path.is_absolute() {
        vec![raw_path.to_path_buf()]
    } else {
        let cwd_path = std::path::Path::new(cwd);
        let base = if cwd_path.is_absolute() {
            cwd_path.to_path_buf()
        } else {
            project_root.join(cwd_path)
        };
        vec![base.join(raw_path), project_root.join(raw_path)]
    };

    candidates.into_iter().find_map(|candidate| {
        let canonical = std::fs::canonicalize(candidate).ok()?;
        canonical.starts_with(&canonical_root).then_some(canonical)
    })
}

fn read_post_compact_file_excerpt(path: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let normalized = content.replace("\r\n", "\n");
    let mut excerpt = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    excerpt = excerpt
        .chars()
        .take(POST_COMPACT_FILE_EXCERPT_MAX_CHARS)
        .collect::<String>();
    if normalized.chars().count() > POST_COMPACT_FILE_EXCERPT_MAX_CHARS {
        excerpt.push_str(" ... [file excerpt truncated]");
    }
    Some(excerpt.trim_end().to_string()).filter(|value| !value.trim().is_empty())
}

fn render_post_compact_file_excerpts(
    project_root: &std::path::Path,
    cwd: &str,
    read_files: &[String],
    preserved_read_files: &std::collections::HashSet<String>,
) -> Vec<String> {
    read_files
        .iter()
        .filter(|file_path| !preserved_read_files.contains(*file_path))
        .cloned()
        .filter_map(|file_path| {
            let resolved = resolve_post_compact_file_path(project_root, cwd, &file_path)?;
            let excerpt = read_post_compact_file_excerpt(&resolved)?;
            let display_path = resolved
                .strip_prefix(project_root)
                .map(|path| path.display().to_string())
                .unwrap_or_else(|_| file_path.clone());
            Some(format!("- Excerpt from {}: {}", display_path, excerpt))
        })
        .take(POST_COMPACT_FILE_EXCERPT_MAX_FILES)
        .collect()
}

fn collect_preserved_read_file_paths(messages: &[Message]) -> std::collections::HashSet<String> {
    let tool_results = messages
        .iter()
        .filter(|message| matches!(message.role, Role::Tool))
        .filter_map(|message| message.tool_call_id.as_deref())
        .collect::<std::collections::HashSet<_>>();
    messages
        .iter()
        .flat_map(|message| message.tool_calls.iter())
        .filter(|call| call.name == "read_file" && tool_results.contains(call.id.as_str()))
        .filter_map(|call| {
            serde_json::from_str::<serde_json::Value>(&call.arguments)
                .ok()?
                .get("file_path")?
                .as_str()
                .map(str::to_string)
        })
        .collect()
}

fn prompt_cache_value(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn prompt_cache_text_value(value: Option<&String>) -> &str {
    value.map(|value| value.as_str()).unwrap_or("none")
}

fn compact_summary_fingerprint(summary: Option<&String>) -> Option<String> {
    let summary = summary?.trim();
    if summary.is_empty() {
        return None;
    }

    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(summary.as_bytes());
    Some(format!("{:x}", hasher.finalize())[..16].to_string())
}

fn message_boundary_key(message: &Message) -> String {
    serde_json::json!({
        "role": format!("{:?}", message.role),
        "content": message.content,
        "reasoning": message.reasoning,
        "tool_calls": message.tool_calls,
        "tool_call_id": message.tool_call_id,
    })
    .to_string()
}

fn preserved_tail_range(
    pre_compact_messages: &[Message],
    post_compact_messages: &[Message],
) -> Option<String> {
    let mut pre_index = pre_compact_messages.len();
    let mut post_index = post_compact_messages.len();

    while pre_index > 0 && post_index > 0 {
        let pre_key = message_boundary_key(&pre_compact_messages[pre_index - 1]);
        let post_key = message_boundary_key(&post_compact_messages[post_index - 1]);
        if pre_key != post_key {
            break;
        }
        pre_index -= 1;
        post_index -= 1;
    }

    (pre_index < pre_compact_messages.len())
        .then(|| format!("{}..{}", pre_index, pre_compact_messages.len()))
}

fn push_artifact_path(artifact_paths: &mut Vec<String>, path: Option<&std::path::Path>) {
    let Some(path) = path else {
        return;
    };
    let path = path.display().to_string();
    if !artifact_paths.contains(&path) {
        artifact_paths.push(path);
    }
}

fn message_excerpt_for_compaction(message: &Message, limit: usize) -> Option<String> {
    let role = match message.role {
        Role::System => "System",
        Role::User => "User",
        Role::Assistant => "Assistant",
        Role::Tool => "Tool",
    };

    let mut body = String::new();
    if let Some(content) = message.content.as_deref() {
        let squashed = content.split_whitespace().collect::<Vec<_>>().join(" ");
        if !squashed.is_empty() {
            body.push_str(&squashed.chars().take(limit).collect::<String>());
            if squashed.chars().count() > limit {
                body.push_str("...");
            }
        }
    }

    if !message.tool_calls.is_empty() {
        if !body.is_empty() {
            body.push_str(" | ");
        }
        let names = message
            .tool_calls
            .iter()
            .map(|call| call.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        body.push_str(&format!("tool_calls: {}", names));
    }

    if let Some(tool_call_id) = message.tool_call_id.as_deref() {
        if !body.is_empty() {
            body.push_str(" | ");
        }
        body.push_str(&format!("tool_result_for: {}", tool_call_id));
    }

    (!body.trim().is_empty()).then(|| format!("{}: {}", role, body.trim()))
}

fn render_removed_messages_for_summary(messages: &[Message], char_budget: usize) -> String {
    let mut rendered = String::new();
    for line in messages
        .iter()
        .filter_map(|message| message_excerpt_for_compaction(message, 420))
    {
        let line_with_break = format!("- {}\n", line);
        if rendered.chars().count() + line_with_break.chars().count() > char_budget {
            break;
        }
        rendered.push_str(&line_with_break);
    }
    rendered
}

fn truncate_head_for_summary_retry(messages: &[Message], error_text: &str) -> Vec<Message> {
    if messages.len() <= 2 {
        return Vec::new();
    }

    if let Some(token_gap) = parse_prompt_too_long_token_gap(error_text) {
        let mut accumulated = 0usize;
        let mut drop_count = 0usize;
        for message in messages {
            accumulated = accumulated.saturating_add(message.estimated_char_count() / 4);
            drop_count = drop_count.saturating_add(1);
            if accumulated >= token_gap {
                break;
            }
        }
        if drop_count > 0 && drop_count < messages.len() {
            return messages.iter().skip(drop_count).cloned().collect();
        }
    }

    let drop_count = (messages.len() / 5).max(1);
    messages.iter().skip(drop_count).cloned().collect()
}

fn collect_assistant_tool_call_ids(messages: &[Message]) -> std::collections::HashSet<String> {
    messages
        .iter()
        .filter(|message| matches!(message.role, Role::Assistant))
        .flat_map(|message| message.tool_calls.iter().map(|call| call.id.clone()))
        .collect()
}

fn collect_tool_result_ids(messages: &[Message]) -> std::collections::HashSet<String> {
    messages
        .iter()
        .filter(|message| matches!(message.role, Role::Tool))
        .filter_map(|message| message.tool_call_id.clone())
        .collect()
}

fn build_fallback_compaction_summary(
    removed_messages: &[Message],
    turn_artifact_path: Option<&str>,
) -> String {
    let mut lines = vec![
        format!(
            "{} Older conversation was compacted to preserve working context.",
            SESSION_MEMORY_SUMMARY_PREFIX
        ),
        format!("- Removed messages: {}", removed_messages.len()),
    ];

    if let Some(path) = turn_artifact_path.filter(|path| !path.trim().is_empty()) {
        lines.push(format!("- Turn artifact: {}", path));
    }

    let highlights = removed_messages
        .iter()
        .filter_map(|message| message_excerpt_for_compaction(message, 140))
        .take(4)
        .collect::<Vec<_>>();
    if !highlights.is_empty() {
        lines.push("- Highlights:".to_string());
        for highlight in highlights {
            lines.push(format!("  - {}", highlight));
        }
    }

    let mut summary = lines.join("\n");
    if summary.chars().count() > SESSION_MEMORY_SUMMARY_MAX_CHARS {
        summary = summary
            .chars()
            .take(SESSION_MEMORY_SUMMARY_MAX_CHARS)
            .collect::<String>();
        summary.push_str("...");
    }
    summary
}

fn format_llm_compaction_summary_content(raw: &str) -> Option<String> {
    let mut content = raw.trim().to_string();
    if content.is_empty() {
        return None;
    }

    let analysis_re = Regex::new(r"(?is)<analysis>.*?</analysis>").ok()?;
    content = analysis_re.replace_all(&content, "").to_string();

    let summary_re = Regex::new(r"(?is)<summary>(.*?)</summary>").ok()?;
    if let Some(captures) = summary_re.captures(&content) {
        content = captures
            .get(1)
            .map(|matched| matched.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
    }

    let blank_re = Regex::new(r"\n{3,}").ok()?;
    content = blank_re.replace_all(content.trim(), "\n\n").to_string();

    (!content.trim().is_empty()).then(|| content.trim().to_string())
}

fn write_post_compact_restore_artifact(
    project_root: &std::path::Path,
    session_id: &str,
    mode: &str,
    blocks: &[(RestoreBlockKind, String)],
    compact_boundary: Option<&CompactBoundaryRuntimeState>,
) -> Option<std::path::PathBuf> {
    let dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-post-compact-restore.md", short_session));

    let mut body = format!(
        "# Post-compact Restore\n\n- Session: {}\n- Mode: {}\n- Timestamp: {}\n\n",
        session_id,
        mode,
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );

    for (_kind, content) in blocks {
        body.push_str("## Block\n\n```text\n");
        body.push_str(content.trim());
        body.push_str("\n```\n\n");
    }
    if let Some(boundary) = compact_boundary {
        body.push_str("## Compact Boundary\n\n```json\n");
        body.push_str(&serde_json::to_string_pretty(boundary).ok()?);
        body.push_str("\n```\n\n");
    }

    std::fs::write(&path, body).ok()?;
    Some(path)
}

fn write_post_compact_restore_state_artifact(
    project_root: &std::path::Path,
    session_id: &str,
    mode: &str,
    blocks: &[(RestoreBlockKind, String)],
    compact_boundary: Option<&CompactBoundaryRuntimeState>,
) -> Option<std::path::PathBuf> {
    let dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-post-compact-restore-state.json", short_session));

    let payload = serde_json::json!({
        "session_id": session_id,
        "mode": mode,
        "updated_at": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        "compact_boundary": compact_boundary,
        "blocks": blocks
            .iter()
            .map(|(kind, content)| RestoreBlockArtifact {
                kind: kind.label(),
                content,
                fingerprint: format!("{:016x}", {
                    use std::hash::{Hash, Hasher};
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    content.hash(&mut hasher);
                    hasher.finish()
                }),
            })
            .collect::<Vec<_>>(),
    });

    std::fs::write(&path, serde_json::to_string_pretty(&payload).ok()?).ok()?;
    Some(path)
}

fn write_post_compact_restore_diff_artifact(
    project_root: &std::path::Path,
    session_id: &str,
    previous: &[(RestoreBlockKind, String)],
    current: &[(RestoreBlockKind, String)],
) -> Option<std::path::PathBuf> {
    if previous == current {
        return None;
    }

    let previous_map = previous
        .iter()
        .map(|(kind, content)| (*kind, content))
        .collect::<std::collections::BTreeMap<_, _>>();
    let current_map = current
        .iter()
        .map(|(kind, content)| (*kind, content))
        .collect::<std::collections::BTreeMap<_, _>>();

    let dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-post-compact-restore-diff.md", short_session));

    let mut body = format!(
        "# Post-compact Restore Diff\n\n- Session: {}\n- Timestamp: {}\n\n",
        session_id,
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );

    for kind in [
        RestoreBlockKind::Runtime,
        RestoreBlockKind::Files,
        RestoreBlockKind::Plan,
        RestoreBlockKind::Tools,
        RestoreBlockKind::PromptCache,
        RestoreBlockKind::Skills,
        RestoreBlockKind::Mcp,
        RestoreBlockKind::Artifacts,
    ] {
        let previous_value = previous_map.get(&kind).map_or("", |value| value.as_str());
        let current_value = current_map.get(&kind).map_or("", |value| value.as_str());
        if previous_value == current_value {
            continue;
        }

        body.push_str(&format!("## {}\n\n", kind.label()));
        body.push_str("### Previous\n\n```text\n");
        body.push_str(if previous_value.is_empty() {
            "(none)"
        } else {
            previous_value.trim()
        });
        body.push_str("\n```\n\n### Current\n\n```text\n");
        body.push_str(if current_value.is_empty() {
            "(none)"
        } else {
            current_value.trim()
        });
        body.push_str("\n```\n\n");
    }

    std::fs::write(&path, body).ok()?;
    Some(path)
}

fn load_post_compact_restore_state_artifact(
    project_root: &std::path::Path,
    session_id: &str,
) -> Option<Vec<(RestoreBlockKind, String)>> {
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = project_root
        .join(".yode")
        .join("status")
        .join(format!("{}-post-compact-restore-state.json", short_session));
    let value =
        serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(path).ok()?).ok()?;
    let blocks = value.get("blocks")?.as_array()?;
    let mut restored = Vec::new();
    for block in blocks {
        let owned = serde_json::from_value::<OwnedRestoreBlockArtifact>(block.clone()).ok()?;
        let kind = match owned.kind.as_str() {
            "runtime" => RestoreBlockKind::Runtime,
            "files" => RestoreBlockKind::Files,
            "plan" => RestoreBlockKind::Plan,
            "tools" => RestoreBlockKind::Tools,
            "prompt-cache" | "prompt_cache" => RestoreBlockKind::PromptCache,
            "skills" => RestoreBlockKind::Skills,
            "mcp" => RestoreBlockKind::Mcp,
            "artifacts" => RestoreBlockKind::Artifacts,
            _ => continue,
        };
        restored.push((kind, owned.content));
    }
    (!restored.is_empty()).then_some(restored)
}

fn restore_block_kind_from_content(content: &str) -> Option<RestoreBlockKind> {
    if content.starts_with(POST_COMPACT_RUNTIME_PREFIX) {
        Some(RestoreBlockKind::Runtime)
    } else if content.starts_with(POST_COMPACT_FILES_PREFIX) {
        Some(RestoreBlockKind::Files)
    } else if content.starts_with(POST_COMPACT_PLAN_PREFIX) {
        Some(RestoreBlockKind::Plan)
    } else if content.starts_with(POST_COMPACT_TOOLS_PREFIX) {
        Some(RestoreBlockKind::Tools)
    } else if content.starts_with(POST_COMPACT_PROMPT_CACHE_PREFIX) {
        Some(RestoreBlockKind::PromptCache)
    } else if content.starts_with(POST_COMPACT_SKILLS_PREFIX) {
        Some(RestoreBlockKind::Skills)
    } else if content.starts_with(POST_COMPACT_MCP_PREFIX) {
        Some(RestoreBlockKind::Mcp)
    } else if content.starts_with(POST_COMPACT_ARTIFACTS_PREFIX) {
        Some(RestoreBlockKind::Artifacts)
    } else {
        None
    }
}

fn restore_block_body(content: &str) -> &str {
    content
        .split_once('\n')
        .map(|(_, body)| body.trim())
        .filter(|body| !body.is_empty())
        .unwrap_or_else(|| content.trim())
}

fn ordered_restore_block_contents(
    contents: impl IntoIterator<Item = (RestoreBlockKind, String)>,
) -> Vec<String> {
    let mut ordered = std::collections::BTreeMap::<RestoreBlockKind, String>::new();
    for (kind, content) in contents {
        if !content.trim().is_empty() {
            ordered.insert(kind, content);
        }
    }
    ordered.into_values().collect()
}

fn sanitize_restore_block_for_request(kind: RestoreBlockKind, content: &str) -> Option<String> {
    let body_lines = content
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    let lines = match kind {
        RestoreBlockKind::Runtime => {
            let mut lines = vec![POST_COMPACT_RUNTIME_PREFIX.to_string()];
            for line in body_lines {
                if line.starts_with("- Runtime cwd:")
                    || line.starts_with("- Persistent memory and instruction context")
                {
                    lines.push(line.to_string());
                }
            }
            if lines.len() == 1 {
                lines.push(
                    "- Persistent memory and instruction context remain available via the system prompt."
                        .to_string(),
                );
            }
            lines
        }
        RestoreBlockKind::Files => {
            let mut lines = vec![POST_COMPACT_FILES_PREFIX.to_string()];
            for line in body_lines {
                if line.starts_with("- Recent files read:")
                    || line.starts_with("- Recent files modified:")
                    || line.starts_with("- Recent file excerpts:")
                    || line.starts_with("- Excerpt from ")
                    || line.starts_with("- No recent file context to restore.")
                {
                    lines.push(line.to_string());
                }
            }
            if lines.len() == 1 {
                lines.push("- No recent file context to restore.".to_string());
            }
            lines
        }
        RestoreBlockKind::Plan => {
            let mut lines = vec![POST_COMPACT_PLAN_PREFIX.to_string()];
            let plan_line = body_lines
                .into_iter()
                .find(|line| line.starts_with("- Plan mode:"))
                .unwrap_or("- Plan mode: unknown");
            lines.push(plan_line.to_string());
            lines
        }
        RestoreBlockKind::Tools => vec![
            POST_COMPACT_TOOLS_PREFIX.to_string(),
            "- Tool availability follows the current runtime tool pool and permission state."
                .to_string(),
        ],
        RestoreBlockKind::PromptCache => {
            let mut lines = vec![POST_COMPACT_PROMPT_CACHE_PREFIX.to_string()];
            for line in body_lines {
                if line.starts_with("- Last turn:")
                    || line.starts_with("- Totals:")
                    || line.starts_with("- Active cache edits:")
                    || line.starts_with("- Expected next drop:")
                    || line.starts_with("- Last break:")
                    || line.starts_with("- Last transition:")
                    || line.starts_with("- Last hashes:")
                {
                    lines.push(line.to_string());
                }
            }
            if lines.len() == 1 {
                lines.push(
                    "- Prompt cache state will be re-derived from the next request.".to_string(),
                );
            }
            lines
        }
        RestoreBlockKind::Skills => {
            let mut lines = vec![POST_COMPACT_SKILLS_PREFIX.to_string()];
            let mut found = false;
            for line in body_lines {
                if line.starts_with("- Path-gated active skills:")
                    || line.starts_with("- Available skills:")
                    || line.starts_with("- No skills discovered.")
                {
                    lines.push(line.to_string());
                    found = true;
                }
            }
            if !found {
                lines.push("- No skills discovered.".to_string());
            }
            lines
        }
        RestoreBlockKind::Mcp => vec![
            POST_COMPACT_MCP_PREFIX.to_string(),
            "- MCP availability follows the current runtime inventory.".to_string(),
        ],
        RestoreBlockKind::Artifacts => vec![
            POST_COMPACT_ARTIFACTS_PREFIX.to_string(),
            "- Compaction and runtime artifacts remain available via status inspectors."
                .to_string(),
        ],
    };

    Some(lines.join("\n"))
}

fn sanitized_request_restore_block_contents(
    contents: impl IntoIterator<Item = String>,
) -> Vec<String> {
    let mut ordered = std::collections::BTreeMap::<RestoreBlockKind, String>::new();
    for content in contents {
        let Some(kind) = restore_block_kind_from_content(&content) else {
            continue;
        };
        if let Some(sanitized) = sanitize_restore_block_for_request(kind, &content) {
            ordered.insert(kind, sanitized);
        }
    }
    ordered.into_values().collect()
}

fn render_hidden_post_compact_restore_prompt(
    contents: impl IntoIterator<Item = RestoreSystemBlockHint>,
) -> Option<String> {
    let restore_blocks = contents.into_iter().collect::<Vec<_>>();

    if restore_blocks.is_empty() {
        return None;
    }

    let mut hidden_restore = format!(
        "{}\n\n\
         Treat the following blocks as re-injected continuation state from earlier context compaction.\n\
         Use them as current session state, but do not repeat them unless they are relevant.\n",
        HIDDEN_POST_COMPACT_RESTORE_PREFIX
    );
    for block in restore_blocks {
        hidden_restore.push_str(&format!("\n## {}\n{}\n", block.kind, block.content.trim()));
    }

    Some(hidden_restore.trim().to_string())
}

impl AgentEngine {
    fn take_post_compact_restore_messages_from_conversation(
        &mut self,
    ) -> Vec<(RestoreBlockKind, String)> {
        let mut extracted = Vec::new();
        self.messages.retain(|message| {
            let Some(content) = message.content.as_deref() else {
                return true;
            };
            let Some(kind) = restore_block_kind_from_content(content) else {
                return true;
            };
            extracted.push((kind, content.to_string()));
            false
        });
        extracted
    }

    fn set_post_compact_restore_blocks(&mut self, contents: Vec<(RestoreBlockKind, String)>) {
        self.post_compact_restore_blocks = ordered_restore_block_contents(contents);
    }

    pub(super) fn request_restore_system_blocks(&self) -> Vec<RestoreSystemBlockHint> {
        sanitized_request_restore_block_contents(self.post_compact_restore_blocks.clone())
            .into_iter()
            .filter_map(|content| {
                let kind = restore_block_kind_from_content(&content)?;
                Some(RestoreSystemBlockHint {
                    kind: kind.label().to_string(),
                    content: restore_block_body(&content).to_string(),
                })
            })
            .collect()
    }

    pub(super) fn hidden_post_compact_restore_prompt_text(&self) -> Option<String> {
        render_hidden_post_compact_restore_prompt(self.request_restore_system_blocks())
    }

    pub(super) fn rehydrate_post_compact_restore_messages(&mut self) {
        let extracted = self.take_post_compact_restore_messages_from_conversation();
        if !extracted.is_empty() {
            self.set_post_compact_restore_blocks(extracted);
        }

        let has_summary_anchor = self.messages.iter().any(|message| {
            matches!(message.role, Role::System)
                && message
                    .content
                    .as_deref()
                    .unwrap_or_default()
                    .starts_with(SESSION_MEMORY_SUMMARY_PREFIX)
        });
        if !has_summary_anchor {
            self.post_compact_restore_blocks.clear();
            return;
        }

        if !self.post_compact_restore_blocks.is_empty() {
            return;
        }

        if let Some(blocks) = load_post_compact_restore_state_artifact(
            &self.context.working_dir_compat(),
            &self.context.session_id,
        ) {
            self.set_post_compact_restore_blocks(blocks);
        }
    }

    pub(super) fn apply_microcompact(&mut self) {
        let media_report = self
            .context_manager
            .microcompact_old_media(&mut self.messages);
        let media_changed = media_report.media_removed > 0;
        self.last_microcompact_media_removed = media_report.media_removed as u32;
        self.last_microcompact_media_saved_chars = media_report.saved_chars as u64;
        if media_changed {
            self.microcompact_media_removed_total = self
                .microcompact_media_removed_total
                .saturating_add(media_report.media_removed as u64);
            self.microcompact_media_saved_chars_total = self
                .microcompact_media_saved_chars_total
                .saturating_add(media_report.saved_chars as u64);
        }

        if self.supports_anthropic_cache_editing() {
            let refs = self
                .context_manager
                .collect_microcompact_cache_refs(&self.messages);
            if refs.is_empty() {
                self.cached_microcompact_deleted_refs.clear();
                self.pending_cache_edit_refs.clear();
                self.prompt_cache_runtime.last_turn_cache_edit_deletions = Some(0);
                if media_changed {
                    self.record_compaction_cause("microcompact_media");
                    self.sync_persisted_messages_snapshot();
                    debug!(
                        "Applied media microcompact: removed {} old attachment(s) and saved ~{} chars",
                        media_report.media_removed, media_report.saved_chars
                    );
                }
                return;
            }
            self.cached_microcompact_deleted_refs = refs.clone();
            self.pending_cache_edit_refs = refs
                .into_iter()
                .filter(|cache_ref| !self.pinned_cache_edit_refs.contains(cache_ref))
                .collect();
            let deletions =
                self.pending_cache_edit_refs
                    .len()
                    .saturating_add(self.pinned_cache_edit_refs.len()) as u32;
            self.prompt_cache_runtime.last_turn_cache_edit_deletions = Some(deletions);
            self.prompt_cache_runtime.cache_edit_turns =
                self.prompt_cache_runtime.cache_edit_turns.saturating_add(1);
            self.prompt_cache_runtime.cache_edit_deletions_total = self
                .prompt_cache_runtime
                .cache_edit_deletions_total
                .saturating_add(deletions as u64);
            self.prompt_cache_runtime.pending_cache_edit_refs =
                self.pending_cache_edit_refs.len() as u32;
            self.prompt_cache_runtime.pinned_cache_edit_refs =
                self.pinned_cache_edit_refs.len() as u32;
            self.record_compaction_cause("microcompact_cached");
            if media_changed {
                self.record_compaction_cause("microcompact_media");
                self.sync_persisted_messages_snapshot();
                debug!(
                    "Applied media microcompact: removed {} old attachment(s) and saved ~{} chars",
                    media_report.media_removed, media_report.saved_chars
                );
            }
            debug!(
                "Prepared cached microcompact with {} total cache references ({} pending, {} pinned)",
                self.cached_microcompact_deleted_refs.len(),
                self.pending_cache_edit_refs.len(),
                self.pinned_cache_edit_refs.len()
            );
            return;
        }

        let report = self.context_manager.microcompact(&mut self.messages);
        if report.tool_results_cleared == 0 && !media_changed {
            return;
        }

        self.cached_microcompact_deleted_refs.clear();
        self.pending_cache_edit_refs.clear();
        self.pinned_cache_edit_refs.clear();
        self.prompt_cache_runtime.last_turn_cache_edit_deletions = Some(0);
        self.prompt_cache_runtime.pending_cache_edit_refs = 0;
        self.prompt_cache_runtime.pinned_cache_edit_refs = 0;
        if report.tool_results_cleared > 0 {
            self.record_compaction_cause("microcompact");
        }
        if media_changed {
            self.record_compaction_cause("microcompact_media");
        }
        self.sync_persisted_messages_snapshot();
        debug!(
            "Applied microcompact: cleared {} older tool results, removed {} old attachment(s), and saved ~{} chars",
            report.tool_results_cleared,
            media_report.media_removed,
            report.saved_chars.saturating_add(media_report.saved_chars)
        );
    }

    pub(super) async fn maybe_compact_context(
        &mut self,
        prompt_tokens: u32,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) {
        let _ = self
            .compact_context(prompt_tokens, event_tx, CompactionMode::Auto, None)
            .await;
    }

    pub(super) async fn reactive_compact_context_for_text(
        &mut self,
        error_text: &str,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) -> bool {
        if self.reactive_compact_attempted {
            return false;
        }
        self.reactive_compact_attempted = true;

        if let Some(end) = self.reactive_prefix_end_for_token_gap(error_text) {
            if self.partial_compact_range(1, end, event_tx).await {
                self.record_compaction_cause("reactive_prefix_compact");
                return true;
            }
        }

        let estimated_tokens = self.estimated_prompt_tokens_for_current_messages();
        self.compact_context(estimated_tokens, event_tx, CompactionMode::Reactive, None)
            .await
    }

    pub(super) fn should_reactive_compact_error(&self, err: &anyhow::Error) -> bool {
        !self.reactive_compact_attempted && is_prompt_too_long_text(&format!("{:#}", err))
    }

    pub(super) fn should_reactive_compact_message(&self, message: &str) -> bool {
        !self.reactive_compact_attempted && is_prompt_too_long_text(message)
    }

    pub(super) fn should_reactive_strip_media_error(&self, err: &anyhow::Error) -> bool {
        !self.reactive_media_strip_attempted && is_media_size_error_text(&format!("{:#}", err))
    }

    pub(super) fn should_reactive_strip_media_message(&self, message: &str) -> bool {
        !self.reactive_media_strip_attempted && is_media_size_error_text(message)
    }

    pub(super) fn reactive_prefix_end_for_token_gap(&self, error_text: &str) -> Option<usize> {
        let gap = parse_prompt_too_long_token_gap(error_text)?;
        let target = gap.saturating_add(REACTIVE_GAP_SAFETY_TOKENS);
        if self.messages.len() <= 8 {
            return None;
        }

        let keep_tail = 8usize;
        let max_end = self.messages.len().saturating_sub(keep_tail);
        if max_end <= 2 {
            return None;
        }

        let mut accumulated = 0usize;
        for idx in 1..max_end {
            accumulated = accumulated.saturating_add(self.messages[idx].estimated_char_count() / 4);
            if accumulated >= target {
                return Some((idx + 1).min(max_end));
            }
        }

        None
    }

    pub(super) fn reactive_strip_old_media(&mut self) -> bool {
        let preserve_recent = 6usize;
        if self.messages.len() <= preserve_recent + 1 {
            return false;
        }

        let mut changed = false;
        let cutoff = self.messages.len().saturating_sub(preserve_recent);
        for message in self.messages.iter_mut().take(cutoff).skip(1) {
            if message.images.is_empty() {
                continue;
            }

            message.images.clear();
            let marker = "[older media removed after API size rejection]";
            match message.content.as_mut() {
                Some(content) if !content.contains(marker) => {
                    content.push_str("\n\n");
                    content.push_str(marker);
                }
                None => {
                    message.content = Some(marker.to_string());
                }
                _ => {}
            }
            message.normalize_in_place();
            changed = true;
        }

        if changed {
            self.reactive_media_strip_attempted = true;
            self.sync_persisted_messages_snapshot();
            self.record_compaction_cause("reactive_strip_media");
        }

        changed
    }

    async fn generate_structured_compaction_summary(
        &self,
        removed_messages: &[Message],
        turn_artifact_path: Option<&str>,
        scope: CompactionSummaryScope,
    ) -> Option<String> {
        if removed_messages.is_empty() || self.provider.name() == "mock" {
            return None;
        }

        let mut retry_messages = removed_messages.to_vec();
        for _attempt in 0..=LLM_COMPACTION_MAX_RETRIES {
            let transcript = render_removed_messages_for_summary(
                &retry_messages,
                LLM_COMPACTION_TRANSCRIPT_CHAR_BUDGET,
            );
            if transcript.trim().is_empty() {
                return None;
            }

            let mut prompt = String::from(
                "CRITICAL: Respond with text only. Do not call tools.\n\
                 Create a structured compaction summary for an AI coding session.\n\
                 You may draft private reasoning in <analysis>...</analysis>, but only the <summary> content will be kept.\n\
                 Return an optional <analysis> block followed by a <summary> block containing markdown.\n\
                 Keep only verified facts.\n\
                 Keep it concise but complete enough to continue work after compaction.\n\
                 In the <summary> block, use exactly these 9 sections in order:\n\
                 1. Goals\n2. Current State\n3. Findings\n4. Decisions\n5. Files\n6. Tools\n7. Constraints\n8. Open Questions\n9. Next Steps\n\
                 Use bullet lists.\n\
                 Use `- None` for empty sections.\n\
                 Do not mention this instruction block.\n\n",
            );
            if let Some(path) = turn_artifact_path.filter(|path| !path.trim().is_empty()) {
                prompt.push_str(&format!("Turn artifact: {}\n\n", path));
            }
            prompt.push_str(scope.prompt_guidance());
            prompt.push_str("\n\n");
            prompt.push_str("Compacted transcript excerpt:\n");
            prompt.push_str(&transcript);

            let request = ChatRequest {
                model: self.context.model.clone(),
                messages: vec![
                    Message::system(
                        "You create compact structured summaries for long coding sessions. Return markdown only.",
                    ),
                    Message::user(prompt),
                ],
                tools: vec![],
                temperature: Some(0.1),
                max_tokens: Some(self.context.get_max_tokens().max(8_192)),
                provider_hints: yode_llm::types::ProviderRequestHints::default(),
            };

            match tokio::time::timeout(
                std::time::Duration::from_secs(
                    crate::constants::timeouts::LLM_COMPACTION_SUMMARY_SECS,
                ),
                self.provider.chat(request),
            )
            .await
            {
                Ok(Ok(response)) => {
                    let content =
                        format_llm_compaction_summary_content(&response.message.content?)?;
                    let mut summary = format!(
                        "{} LLM-generated structured summary of compacted conversation.\n{}",
                        SESSION_MEMORY_SUMMARY_PREFIX, content
                    );
                    if summary.chars().count() > LLM_COMPACTION_SUMMARY_MAX_CHARS {
                        summary = summary
                            .chars()
                            .take(LLM_COMPACTION_SUMMARY_MAX_CHARS)
                            .collect::<String>();
                        summary.push_str("...");
                    }
                    return Some(summary);
                }
                Ok(Err(err)) if is_prompt_too_long_text(&format!("{:#}", err)) => {
                    let truncated =
                        truncate_head_for_summary_retry(&retry_messages, &format!("{:#}", err));
                    if truncated.is_empty() {
                        return None;
                    }
                    retry_messages = truncated;
                    continue;
                }
                Ok(Err(err)) => {
                    warn!("Failed to generate structured compaction summary: {}", err);
                    return None;
                }
                Err(_) => {
                    warn!("Timed out while generating structured compaction summary");
                    return None;
                }
            }
        }

        None
    }

    fn replace_compaction_summary_message(
        &mut self,
        previous_summary: Option<&str>,
        new_summary: &str,
    ) {
        if let Some(previous_summary) = previous_summary {
            if let Some(message) = self.messages.iter_mut().find(|message| {
                matches!(message.role, Role::System)
                    && message.content.as_deref() == Some(previous_summary)
            }) {
                *message = Message::system(new_summary.to_string());
                return;
            }
        }

        if let Some(message) = self.messages.iter_mut().rev().find(|message| {
            matches!(message.role, Role::System)
                && message
                    .content
                    .as_deref()
                    .unwrap_or_default()
                    .starts_with(SESSION_MEMORY_SUMMARY_PREFIX)
        }) {
            *message = Message::system(new_summary.to_string());
        }
    }

    async fn build_post_compact_restore_messages(
        &self,
        mode: CompactionMode,
        session_memory_path: Option<&std::path::Path>,
        transcript_path: Option<&std::path::Path>,
        post_compact_estimated_tokens: Option<u32>,
        auto_compact_threshold: Option<u32>,
        will_retrigger_next_turn: Option<bool>,
    ) -> Vec<(RestoreBlockKind, String)> {
        let project_root = self.context.working_dir_compat();
        let cwd = self.current_runtime_working_dir().await;
        let tool_pool = self.build_tool_pool_snapshot();
        let inventory = self.tools.inventory();
        let plan_mode_enabled = *self.plan_mode.lock().await;
        let skills = crate::skills::SkillRegistry::discover(
            &crate::skills::SkillRegistry::default_paths(&project_root),
        );
        let mcp_cache = yode_tools::mcp_resource_cache_stats();

        let read_files = ordered_recent_read_files(&self.recent_file_reads, &self.files_read);
        let modified_files = self.files_modified.clone();
        let recent_paths = read_files
            .iter()
            .chain(modified_files.iter())
            .cloned()
            .collect::<Vec<_>>();
        let active_skills = skills.active_for_paths(recent_paths.iter());
        let skill_names = skills
            .list()
            .iter()
            .take(5)
            .map(|skill| skill.name.clone())
            .collect::<Vec<_>>();

        let mut runtime_lines = vec![
            format!(
                "{} Re-injected runtime context after {} compaction.",
                POST_COMPACT_RUNTIME_PREFIX,
                mode.label()
            ),
            format!("- Runtime cwd: {}", cwd),
            "- Persistent memory and instruction context remain available via the system prompt."
                .to_string(),
        ];
        if let (Some(estimated_tokens), Some(threshold), Some(will_retrigger)) = (
            post_compact_estimated_tokens,
            auto_compact_threshold,
            will_retrigger_next_turn,
        ) {
            let delta = estimated_tokens as i64 - threshold as i64;
            runtime_lines.push(format!(
                "- Post-compact pressure: est={} threshold={} delta={} next_auto={}",
                estimated_tokens,
                threshold,
                delta,
                if will_retrigger { "likely" } else { "clear" }
            ));
        }

        let mut file_lines = vec![POST_COMPACT_FILES_PREFIX.to_string()];
        if let Some(summary) = summarize_string_entries(&read_files, 5) {
            file_lines.push(format!("- Recent files read: {}", summary));
        }
        if let Some(summary) = summarize_string_entries(&modified_files, 5) {
            file_lines.push(format!("- Recent files modified: {}", summary));
        }
        let preserved_read_files = collect_preserved_read_file_paths(&self.messages);
        let file_excerpts = render_post_compact_file_excerpts(
            &project_root,
            &cwd,
            &read_files,
            &preserved_read_files,
        );
        if !file_excerpts.is_empty() {
            file_lines.push("- Recent file excerpts:".to_string());
            file_lines.extend(file_excerpts);
        }
        let skipped_files = read_files
            .iter()
            .filter(|path| preserved_read_files.contains(*path))
            .take(3)
            .cloned()
            .collect::<Vec<_>>();
        if let Some(summary) = summarize_string_entries(&skipped_files, 3) {
            file_lines.push(format!(
                "- Skipped excerpts already preserved in tail: {}",
                summary
            ));
        }
        if file_lines.len() == 1 {
            file_lines.push("- No recent file context to restore.".to_string());
        }

        let mut plan_lines = vec![POST_COMPACT_PLAN_PREFIX.to_string()];
        plan_lines.push(format!(
            "- Plan mode: {}",
            if plan_mode_enabled {
                "enabled"
            } else {
                "disabled"
            }
        ));

        let mut tool_lines = vec![POST_COMPACT_TOOLS_PREFIX.to_string()];
        tool_lines.push(format!(
            "- Tool pool: {} active visible, {} active hidden, {} deferred visible, search={} (reason: {})",
            tool_pool.visible_active_count(),
            tool_pool.hidden_active_count(),
            tool_pool.visible_deferred_count(),
            if tool_pool.tool_search_enabled { "enabled" } else { "disabled" },
            tool_pool.tool_search_reason.as_deref().unwrap_or("none")
        ));
        tool_lines.push(format!(
            "- Tool inventory: total={} active={} deferred={} activations={} last={}",
            inventory.total_count,
            inventory.active_count,
            inventory.deferred_count,
            inventory.activation_count,
            inventory.last_activated_tool.as_deref().unwrap_or("none")
        ));

        let cache = &self.prompt_cache_runtime;
        let prompt_cache_lines = [
            POST_COMPACT_PROMPT_CACHE_PREFIX.to_string(),
            format!(
                "- Last turn: prompt={} completion={} write={} read={} edit_del={}",
                prompt_cache_value(cache.last_turn_prompt_tokens),
                prompt_cache_value(cache.last_turn_completion_tokens),
                prompt_cache_value(cache.last_turn_cache_write_tokens),
                prompt_cache_value(cache.last_turn_cache_read_tokens),
                prompt_cache_value(cache.last_turn_cache_edit_deletions)
            ),
            format!(
                "- Totals: turns={} write={} read={} edit_deletions={} deleted_tokens={}",
                cache.reported_turns,
                cache.cache_write_tokens_total,
                cache.cache_read_tokens_total,
                cache.cache_edit_deletions_total,
                cache.cache_deleted_tokens_total
            ),
            format!(
                "- Active cache edits: pending={} pinned={}",
                self.pending_cache_edit_refs.len(),
                self.pinned_cache_edit_refs.len()
            ),
            format!("- Expected next drop: compaction_{}", mode.label()),
            format!(
                "- Last break: count={} reason={} at={}",
                cache.prompt_cache_break_count,
                cache
                    .last_prompt_cache_break_reason
                    .as_deref()
                    .unwrap_or("none"),
                cache
                    .last_prompt_cache_break_at
                    .as_deref()
                    .unwrap_or("none")
            ),
            format!(
                "- Last transition: kind={} reason={} change={}",
                cache
                    .last_prompt_cache_transition_kind
                    .as_deref()
                    .unwrap_or("none"),
                cache
                    .last_prompt_cache_transition_reason
                    .as_deref()
                    .unwrap_or("none"),
                cache
                    .last_prompt_cache_change_summary
                    .as_deref()
                    .unwrap_or("none")
            ),
            format!(
                "- Last hashes: prefix={} system={} restore={} tool={} message={}",
                prompt_cache_text_value(self.last_prompt_cache_prefix_hash.as_ref()),
                prompt_cache_text_value(self.last_prompt_cache_system_hash.as_ref()),
                prompt_cache_text_value(self.last_prompt_cache_restore_hash.as_ref()),
                prompt_cache_text_value(self.last_prompt_cache_tool_hash.as_ref()),
                prompt_cache_text_value(self.last_prompt_cache_message_hash.as_ref())
            ),
        ];

        let mut mcp_lines = vec![POST_COMPACT_MCP_PREFIX.to_string()];
        mcp_lines.push(format!(
            "- MCP: visible_tools={} deferred_tools={} cache(list {} hit/{} miss, read {} hit/{} miss)",
            tool_pool.visible_mcp_count(),
            inventory.mcp_deferred_count,
            mcp_cache.list_hits,
            mcp_cache.list_misses,
            mcp_cache.read_hits,
            mcp_cache.read_misses
        ));

        let mut skill_lines = vec![POST_COMPACT_SKILLS_PREFIX.to_string()];
        if !active_skills.is_empty() {
            let rendered = active_skills
                .iter()
                .take(5)
                .map(|skill| {
                    if skill.metadata.paths.is_empty() {
                        skill.name.clone()
                    } else {
                        format!(
                            "{} (paths: {})",
                            skill.name,
                            skill.metadata.paths.join(", ")
                        )
                    }
                })
                .collect::<Vec<_>>()
                .join("; ");
            skill_lines.push(format!("- Path-gated active skills: {}", rendered));
        }
        if !skill_names.is_empty() {
            skill_lines.push(format!("- Available skills: {}", skill_names.join(", ")));
        } else {
            skill_lines.push("- No skills discovered.".to_string());
        }

        let mut artifact_lines = vec![POST_COMPACT_ARTIFACTS_PREFIX.to_string()];
        if let Some(path) = session_memory_path {
            artifact_lines.push(format!(
                "- Session memory artifact: {}",
                display_compaction_memory_path(&project_root, path)
            ));
        }
        if let Some(path) = transcript_path {
            artifact_lines.push(format!(
                "- Compaction transcript: {}",
                display_compaction_memory_path(&project_root, path)
            ));
        }
        if let Some(path) = self.last_tool_turn_artifact_path.as_deref() {
            artifact_lines.push(format!("- Latest tool artifact: {}", path));
        }
        if let Some(path) = self.last_turn_artifact_path.as_deref() {
            artifact_lines.push(format!("- Latest turn artifact: {}", path));
        }
        if artifact_lines.len() == 1 {
            artifact_lines.push("- No artifact links available.".to_string());
        }

        vec![
            (RestoreBlockKind::Runtime, runtime_lines.join("\n")),
            (RestoreBlockKind::Files, file_lines.join("\n")),
            (RestoreBlockKind::Plan, plan_lines.join("\n")),
            (RestoreBlockKind::Tools, tool_lines.join("\n")),
            (RestoreBlockKind::PromptCache, prompt_cache_lines.join("\n")),
            (RestoreBlockKind::Skills, skill_lines.join("\n")),
            (RestoreBlockKind::Mcp, mcp_lines.join("\n")),
            (RestoreBlockKind::Artifacts, artifact_lines.join("\n")),
        ]
    }

    async fn finalize_compaction_result(
        &mut self,
        mode: CompactionMode,
        prompt_tokens: u32,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
        pre_compact_messages: Vec<Message>,
        report: CompressionReport,
        used_session_memory: bool,
    ) -> bool {
        let mode_label = mode.label();
        let mut report = report;

        if !used_session_memory && report.removed > 0 {
            if let Some(summary) = self
                .generate_structured_compaction_summary(
                    &report.removed_messages,
                    self.last_turn_artifact_path.as_deref(),
                    CompactionSummaryScope::Full,
                )
                .await
            {
                let previous_summary = report.summary.clone();
                self.replace_compaction_summary_message(previous_summary.as_deref(), &summary);
                report.summary = Some(summary);
            }
        }

        let mut session_memory_path = None;
        let mut transcript_path = None;
        let project_root = self.context.working_dir_compat();
        let compacted_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        match persist_compaction_memory(
            &project_root,
            &self.context.session_id,
            &report,
            &self.files_read,
            &self.files_modified,
        ) {
            Ok(path) => {
                session_memory_path = Some(path);
            }
            Err(err) => warn!("Failed to persist session memory after compaction: {}", err),
        }
        let post_compact_estimated_tokens = self
            .context_manager
            .estimate_tokens_for_messages(&self.messages);
        let auto_compact_threshold = self.context_manager.compression_threshold_tokens();
        let will_retrigger_next_turn = post_compact_estimated_tokens >= auto_compact_threshold;
        self.last_post_compaction_estimated_tokens = Some(post_compact_estimated_tokens as u32);
        self.last_post_compaction_threshold_tokens = Some(auto_compact_threshold as u32);
        self.last_post_compaction_will_retrigger = Some(will_retrigger_next_turn);

        let mut compact_boundary = CompactBoundaryRuntimeState {
            mode: mode_label.to_string(),
            timestamp: compacted_at.clone(),
            removed_count: report.removed,
            tool_results_truncated: report.tool_results_truncated,
            preserved_tail_range: preserved_tail_range(&pre_compact_messages, &self.messages),
            summary_fingerprint: compact_summary_fingerprint(report.summary.as_ref()),
            post_compact_estimated_tokens: post_compact_estimated_tokens as u32,
            post_compact_threshold_tokens: auto_compact_threshold as u32,
            post_compact_token_delta: post_compact_estimated_tokens as i64
                - auto_compact_threshold as i64,
            will_retrigger_next_turn,
            artifact_paths: Vec::new(),
        };
        push_artifact_path(
            &mut compact_boundary.artifact_paths,
            session_memory_path.as_deref(),
        );

        match write_compaction_transcript(
            &project_root,
            &self.context.session_id,
            &pre_compact_messages,
            &report,
            mode_label,
            &self.failed_tool_call_ids,
            session_memory_path.as_deref(),
            &self.files_read,
            &self.files_modified,
            Some(&compact_boundary),
        ) {
            Ok(path) => {
                push_artifact_path(&mut compact_boundary.artifact_paths, Some(&path));
                transcript_path = Some(path);
            }
            Err(err) => warn!("Failed to write compaction transcript: {}", err),
        }

        let restore_messages = self
            .build_post_compact_restore_messages(
                mode,
                session_memory_path.as_deref(),
                transcript_path.as_deref(),
                Some(post_compact_estimated_tokens as u32),
                Some(auto_compact_threshold as u32),
                Some(will_retrigger_next_turn),
            )
            .await;
        let previous_restore_messages =
            load_post_compact_restore_state_artifact(&project_root, &self.context.session_id);
        let restore_artifact_path = write_post_compact_restore_artifact(
            &project_root,
            &self.context.session_id,
            mode_label,
            &restore_messages,
            Some(&compact_boundary),
        );
        let restore_state_artifact_path = write_post_compact_restore_state_artifact(
            &project_root,
            &self.context.session_id,
            mode_label,
            &restore_messages,
            Some(&compact_boundary),
        );
        push_artifact_path(
            &mut compact_boundary.artifact_paths,
            restore_artifact_path.as_deref(),
        );
        push_artifact_path(
            &mut compact_boundary.artifact_paths,
            restore_state_artifact_path.as_deref(),
        );
        if let Some(previous) = previous_restore_messages.as_ref() {
            let _ = write_post_compact_restore_diff_artifact(
                &project_root,
                &self.context.session_id,
                previous,
                &restore_messages,
            );
        }
        self.set_post_compact_restore_blocks(restore_messages);
        self.take_post_compact_restore_messages_from_conversation();
        self.clear_cache_edit_tracking();
        self.sync_persisted_messages_snapshot();

        let post_context = self.build_compaction_hook_context(
            HookEvent::PostCompact,
            mode_label,
            prompt_tokens,
            Some(&report),
            session_memory_path.as_deref(),
            transcript_path.as_deref(),
        );
        self.execute_advisory_hooks(HookEvent::PostCompact, post_context)
            .await;
        let compressed_context = self.build_compaction_hook_context(
            HookEvent::ContextCompressed,
            mode_label,
            prompt_tokens,
            Some(&report),
            session_memory_path.as_deref(),
            transcript_path.as_deref(),
        );
        self.execute_advisory_hooks(HookEvent::ContextCompressed, compressed_context)
            .await;

        let still_above_threshold = self
            .context_manager
            .exceeds_threshold_estimate(&self.messages);
        self.compaction_in_progress = false;

        if still_above_threshold && mode.is_auto() {
            self.record_compaction_cause("failed_above_threshold");
            self.record_compaction_failure(
                "context remains above the safety threshold after compaction",
                event_tx,
            );
        } else if mode.is_auto() {
            self.compaction_failures = 0;
        }

        let session_memory_path_str = session_memory_path
            .as_ref()
            .map(|p| p.display().to_string());
        let transcript_path_str = transcript_path.as_ref().map(|p| p.display().to_string());

        let _ = event_tx.send(EngineEvent::ContextCompressed {
            mode: mode_label.to_string(),
            removed: report.removed,
            tool_results_truncated: report.tool_results_truncated,
            summary: report.summary.clone(),
            session_memory_path: session_memory_path_str.clone(),
            transcript_path: transcript_path_str.clone(),
        });
        self.last_compaction_mode = Some(mode_label.to_string());
        self.last_compaction_at = Some(compacted_at);
        self.last_compaction_summary_excerpt = report.summary.as_ref().map(|summary| {
            let excerpt: String = summary.chars().take(160).collect();
            if summary.chars().count() > 160 {
                format!("{}...", excerpt)
            } else {
                excerpt
            }
        });
        self.last_compaction_session_memory_path = session_memory_path_str;
        self.last_compaction_transcript_path = transcript_path_str;
        self.last_compact_boundary = Some(compact_boundary);
        self.last_compaction_prompt_tokens = Some(prompt_tokens);
        self.compaction_prompt_tokens_total = self
            .compaction_prompt_tokens_total
            .saturating_add(prompt_tokens as u64);
        self.compaction_prompt_token_samples =
            self.compaction_prompt_token_samples.saturating_add(1);
        self.total_compactions = self.total_compactions.saturating_add(1);
        match mode {
            CompactionMode::Auto => {
                self.auto_compactions = self.auto_compactions.saturating_add(1);
                if used_session_memory {
                    self.record_compaction_cause("success_auto_session_memory");
                } else {
                    self.record_compaction_cause("success_auto");
                }
            }
            CompactionMode::Manual => {
                self.manual_compactions = self.manual_compactions.saturating_add(1);
                self.record_compaction_cause("success_manual");
            }
            CompactionMode::Reactive => {
                self.record_compaction_cause("success_reactive");
            }
        }
        self.set_expected_prompt_cache_drop_reason(format!("compaction_{}", mode_label));
        self.persist_session_artifacts();
        true
    }

    async fn compact_context(
        &mut self,
        prompt_tokens: u32,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
        mode: CompactionMode,
        keep_last_override: Option<usize>,
    ) -> bool {
        let mode_label = mode.label();

        if mode.is_auto() && !self.current_query_source.allows_auto_compaction() {
            self.record_compaction_cause("skipped_query_source");
            debug!(
                "Skipping auto-compaction for query source {:?}",
                self.current_query_source
            );
            return false;
        }

        if mode.is_auto() && self.autocompact_disabled {
            self.record_compaction_cause("skipped_breaker_open");
            debug!("Skipping auto-compaction because the circuit breaker is open");
            return false;
        }

        if self.compaction_in_progress {
            self.record_compaction_cause("skipped_nested");
            warn!("Skipping nested compaction attempt");
            return false;
        }

        if mode.is_auto()
            && !self
                .context_manager
                .should_compress(prompt_tokens, &self.messages)
        {
            self.record_compaction_cause("skipped_below_threshold");
            return false;
        }

        self.compaction_in_progress = true;
        let _ = event_tx.send(EngineEvent::ContextCompactionStarted {
            mode: mode_label.to_string(),
        });

        let pre_context = self.build_compaction_hook_context(
            HookEvent::PreCompact,
            mode_label,
            prompt_tokens,
            None,
            None,
            None,
        );
        self.execute_advisory_hooks(HookEvent::PreCompact, pre_context)
            .await;

        let pre_compact_messages = self.messages.clone();
        let (report, used_session_memory) = if let Some(keep_last) = keep_last_override {
            (
                self.context_manager.compress_with_keep_last(
                    &mut self.messages,
                    keep_last,
                    self.last_turn_artifact_path.as_deref(),
                ),
                false,
            )
        } else if mode.is_auto() {
            if let Some(report) = self.try_session_memory_compaction() {
                self.record_compaction_cause("strategy_session_memory");
                (report, true)
            } else {
                (
                    self.context_manager.compress_with_turn_artifact(
                        &mut self.messages,
                        self.last_turn_artifact_path.as_deref(),
                    ),
                    false,
                )
            }
        } else {
            (
                self.context_manager.compress_with_turn_artifact(
                    &mut self.messages,
                    self.last_turn_artifact_path.as_deref(),
                ),
                false,
            )
        };
        if report.removed == 0 && report.tool_results_truncated == 0 {
            self.compaction_in_progress = false;
            match mode {
                CompactionMode::Auto => {
                    self.record_compaction_cause("failed_no_change");
                    self.record_compaction_failure("compression made no changes", event_tx);
                }
                CompactionMode::Reactive => {
                    self.record_compaction_cause("failed_reactive_no_change");
                }
                CompactionMode::Manual => {}
            }
            return false;
        }

        self.finalize_compaction_result(
            mode,
            prompt_tokens,
            event_tx,
            pre_compact_messages,
            report,
            used_session_memory,
        )
        .await
    }

    pub(super) fn estimated_prompt_tokens_for_current_messages(&self) -> u32 {
        let base = self
            .context_manager
            .estimate_tokens_for_messages(&self.messages);
        let restore = self
            .hidden_post_compact_restore_prompt_text()
            .map(|text| {
                self.context_manager
                    .estimate_tokens_for_messages(&[Message::system(text)])
            })
            .unwrap_or(0);
        base.saturating_add(restore).max(1) as u32
    }

    pub async fn force_compact(&mut self, event_tx: mpsc::UnboundedSender<EngineEvent>) -> bool {
        let estimated_tokens = self.estimated_prompt_tokens_for_current_messages();
        self.compact_context(estimated_tokens, &event_tx, CompactionMode::Manual, None)
            .await
    }

    pub async fn force_compact_keep_last(
        &mut self,
        keep_last: usize,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
    ) -> bool {
        let estimated_tokens = self.estimated_prompt_tokens_for_current_messages();
        self.compact_context(
            estimated_tokens,
            &event_tx,
            CompactionMode::Manual,
            Some(keep_last.max(1)),
        )
        .await
    }

    pub async fn force_partial_compact_up_to(
        &mut self,
        up_to: usize,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
    ) -> bool {
        let message_count = self.messages.len().saturating_sub(1);
        if message_count == 0 {
            return false;
        }
        let end = 1 + up_to.min(message_count);
        self.partial_compact_range(1, end, &event_tx).await
    }

    pub async fn force_partial_compact_from(
        &mut self,
        from: usize,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
    ) -> bool {
        let message_count = self.messages.len().saturating_sub(1);
        if message_count == 0 {
            return false;
        }
        let logical_start = from.clamp(1, message_count);
        let start = logical_start;
        self.partial_compact_range(start, self.messages.len(), &event_tx)
            .await
    }

    fn try_session_memory_compaction(&mut self) -> Option<CompressionReport> {
        let project_root = self.context.working_dir_compat();
        let (path, excerpt) = best_compaction_memory_excerpt(&project_root, 900)?;
        let summary = build_session_memory_compaction_summary(&project_root, &path, &excerpt);
        let report =
            self.context_manager
                .compact_with_external_summary(&mut self.messages, 8, summary);
        (report.removed > 0 || report.tool_results_truncated > 0).then_some(report)
    }

    fn expand_partial_compaction_range(
        &self,
        mut start: usize,
        mut end: usize,
    ) -> Option<(usize, usize)> {
        if start < 1 || end > self.messages.len() || start >= end {
            return None;
        }

        loop {
            let range = &self.messages[start..end];
            let summarized_tool_calls = collect_assistant_tool_call_ids(range);
            let summarized_tool_results = collect_tool_result_ids(range);
            let mut changed = false;

            let mut idx = 1;
            while idx < start {
                let message = &self.messages[idx];
                if matches!(message.role, Role::Assistant)
                    && message
                        .tool_calls
                        .iter()
                        .any(|call| summarized_tool_results.contains(&call.id))
                {
                    start = idx;
                    changed = true;
                    break;
                }
                idx += 1;
            }

            let mut idx = end;
            while idx < self.messages.len() {
                let message = &self.messages[idx];
                if matches!(message.role, Role::Tool)
                    && message
                        .tool_call_id
                        .as_ref()
                        .is_some_and(|id| summarized_tool_calls.contains(id))
                {
                    end = idx + 1;
                    changed = true;
                }
                idx += 1;
            }

            if !changed {
                return Some((start, end));
            }
        }
    }

    async fn partial_compact_range(
        &mut self,
        start: usize,
        end: usize,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) -> bool {
        if self.compaction_in_progress {
            self.record_compaction_cause("skipped_nested");
            return false;
        }

        let Some((start, end)) = self.expand_partial_compaction_range(start, end) else {
            return false;
        };
        if start >= end || end > self.messages.len() {
            return false;
        }

        self.compaction_in_progress = true;
        let prompt_tokens = self.estimated_prompt_tokens_for_current_messages();
        let _ = event_tx.send(EngineEvent::ContextCompactionStarted {
            mode: CompactionMode::Manual.label().to_string(),
        });
        let pre_context = self.build_compaction_hook_context(
            HookEvent::PreCompact,
            CompactionMode::Manual.label(),
            prompt_tokens,
            None,
            None,
            None,
        );
        self.execute_advisory_hooks(HookEvent::PreCompact, pre_context)
            .await;

        let pre_compact_messages = self.messages.clone();
        let removed_messages = self.messages[start..end].to_vec();
        if removed_messages.is_empty() {
            self.compaction_in_progress = false;
            return false;
        }

        let summary = self
            .generate_structured_compaction_summary(
                &removed_messages,
                self.last_turn_artifact_path.as_deref(),
                if start <= 1 {
                    CompactionSummaryScope::PartialUpTo
                } else {
                    CompactionSummaryScope::PartialFrom
                },
            )
            .await
            .unwrap_or_else(|| {
                build_fallback_compaction_summary(
                    &removed_messages,
                    self.last_turn_artifact_path.as_deref(),
                )
            });

        self.messages.drain(start..end);
        self.messages
            .insert(start, Message::system(summary.clone()));

        let report = CompressionReport {
            removed: removed_messages.len(),
            tool_results_truncated: 0,
            summary: Some(summary),
            removed_messages,
        };

        self.finalize_compaction_result(
            CompactionMode::Manual,
            prompt_tokens,
            event_tx,
            pre_compact_messages,
            report,
            false,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use yode_llm::types::Message;

    use super::{
        format_llm_compaction_summary_content, parse_prompt_too_long_token_gap,
        truncate_head_for_summary_retry,
    };

    #[test]
    fn parses_prompt_too_long_gap_from_error_text() {
        assert_eq!(
            parse_prompt_too_long_token_gap("prompt is too long: 137500 tokens > 135000 maximum"),
            Some(2500)
        );
        assert_eq!(parse_prompt_too_long_token_gap("something else"), None);
    }

    #[test]
    fn truncate_head_retry_prefers_reported_token_gap() {
        let messages = vec![
            Message::user("x".repeat(8_000)),
            Message::assistant("y".repeat(8_000)),
            Message::user("keep"),
        ];

        let truncated = truncate_head_for_summary_retry(
            &messages,
            "prompt is too long: 6000 tokens > 2000 maximum",
        );

        assert!(truncated.len() < messages.len());
        assert_eq!(
            truncated
                .last()
                .and_then(|message| message.content.as_deref()),
            Some("keep")
        );
    }

    #[test]
    fn formats_llm_compaction_summary_by_stripping_analysis() {
        let raw = "<analysis>\nprivate draft\n</analysis>\n\n<summary>\n## Goals\n- Continue compact parity\n\n\n## Next Steps\n- Run tests\n</summary>";

        let formatted = format_llm_compaction_summary_content(raw).unwrap();

        assert!(!formatted.contains("private draft"));
        assert!(!formatted.contains("<summary>"));
        assert!(formatted.starts_with("## Goals"));
        assert!(formatted.contains("## Next Steps"));
    }
}
