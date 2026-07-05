use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use yode_llm::types::{Message, RestoreSystemBlockHint, Role};
use yode_tools::builtin::skill::SkillInvocation;
use yode_tools::RuntimeTask;

use crate::engine::types::{CompactBoundaryRuntimeState, RestoreBudgetRuntimeState};

pub(super) const POST_COMPACT_RUNTIME_PREFIX: &str = "[Post-compact restore: runtime]";
pub(super) const POST_COMPACT_FILES_PREFIX: &str = "[Post-compact restore: files]";
pub(super) const POST_COMPACT_PLAN_PREFIX: &str = "[Post-compact restore: plan]";
pub(super) const POST_COMPACT_TASKS_PREFIX: &str = "[Post-compact restore: tasks]";
pub(super) const POST_COMPACT_TOOLS_PREFIX: &str = "[Post-compact restore: tools]";
pub(super) const POST_COMPACT_PROMPT_CACHE_PREFIX: &str = "[Post-compact restore: prompt-cache]";
pub(super) const POST_COMPACT_SKILLS_PREFIX: &str = "[Post-compact restore: skills]";
pub(super) const POST_COMPACT_MCP_PREFIX: &str = "[Post-compact restore: mcp]";
pub(super) const POST_COMPACT_ARTIFACTS_PREFIX: &str = "[Post-compact restore: artifacts]";
pub(super) const HIDDEN_POST_COMPACT_RESTORE_PREFIX: &str = "# Post-compact Restore";

const POST_COMPACT_FILE_EXCERPT_MAX_FILES: usize = 3;
const POST_COMPACT_FILE_EXCERPT_MAX_CHARS: usize = 900;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RestoreBlockKind {
    Runtime,
    Files,
    Plan,
    Tasks,
    Tools,
    PromptCache,
    Skills,
    Mcp,
    Artifacts,
}

impl RestoreBlockKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::Files => "files",
            Self::Plan => "plan",
            Self::Tasks => "tasks",
            Self::Tools => "tools",
            Self::PromptCache => "prompt-cache",
            Self::Skills => "skills",
            Self::Mcp => "mcp",
            Self::Artifacts => "artifacts",
        }
    }
}

#[derive(Serialize)]
pub(super) struct RestoreBlockArtifact<'a> {
    pub(super) kind: &'a str,
    pub(super) content: &'a str,
    pub(super) fingerprint: String,
}

#[derive(Deserialize)]
pub(super) struct OwnedRestoreBlockArtifact {
    pub(super) kind: String,
    pub(super) content: String,
    #[serde(rename = "fingerprint")]
    pub(super) _fingerprint: String,
}

pub(super) fn render_skill_invocation_restore_lines(
    invocations: &[SkillInvocation],
) -> Vec<String> {
    if invocations.is_empty() {
        return Vec::new();
    }

    let mut recent = invocations.iter().rev().take(5).collect::<Vec<_>>();
    recent.reverse();
    let mut lines = vec!["- Recently invoked skills:".to_string()];
    for invocation in recent {
        lines.push(format!(
            "  - {} via {}{} — {}",
            invocation.name,
            invocation.action,
            render_skill_invocation_scope(invocation),
            empty_restore_label(&invocation.description)
        ));
        if !invocation.content_excerpt.trim().is_empty() {
            lines.push(format!(
                "    excerpt: {}",
                compact_restore_excerpt(&invocation.content_excerpt, 520)
            ));
        }
        if invocation.content_truncated {
            lines.push(format!(
                "    recovery: run `skill` get {} for the full skill content.",
                invocation.name
            ));
        }
    }
    lines
}

fn render_skill_invocation_scope(invocation: &SkillInvocation) -> String {
    if let (Some(team_id), Some(member_id)) = (
        invocation.team_id.as_deref(),
        invocation.member_id.as_deref(),
    ) {
        return format!(" [team={} member={}]", team_id, member_id);
    }
    if let Some(description) = invocation.subagent_description.as_deref() {
        return format!(" [subagent={}]", description);
    }
    if let Some(session_id) = invocation.session_id.as_deref() {
        return format!(
            " [session={}]",
            session_id.chars().take(8).collect::<String>()
        );
    }
    String::new()
}

fn compact_restore_excerpt(value: &str, max_chars: usize) -> String {
    let mut compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() > max_chars {
        compact = compact.chars().take(max_chars).collect::<String>();
        compact.push_str("...");
    }
    compact
}

fn empty_restore_label(value: &str) -> &str {
    if value.trim().is_empty() {
        "(no description)"
    } else {
        value
    }
}

async fn resolve_post_compact_file_path(
    project_root: &Path,
    cwd: &str,
    file_path: &str,
) -> Option<PathBuf> {
    let file_path = file_path.trim();
    if file_path.is_empty() {
        return None;
    }

    let canonical_root = tokio::fs::canonicalize(project_root).await.ok()?;
    let raw_path = Path::new(file_path);
    let candidates = if raw_path.is_absolute() {
        vec![raw_path.to_path_buf()]
    } else {
        let cwd_path = Path::new(cwd);
        let base = if cwd_path.is_absolute() {
            cwd_path.to_path_buf()
        } else {
            project_root.join(cwd_path)
        };
        vec![base.join(raw_path), project_root.join(raw_path)]
    };

    for candidate in candidates {
        let canonical = tokio::fs::canonicalize(candidate).await.ok()?;
        if canonical.starts_with(&canonical_root) {
            return Some(canonical);
        }
    }
    None
}

async fn read_post_compact_file_excerpt(path: &Path) -> Option<String> {
    let content = tokio::fs::read_to_string(path).await.ok()?;
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

pub(super) async fn render_post_compact_file_excerpts(
    project_root: &Path,
    cwd: &str,
    read_files: &[String],
    preserved_read_files: &HashSet<String>,
) -> Vec<String> {
    let mut excerpts = Vec::new();
    for file_path in read_files
        .iter()
        .filter(|file_path| !preserved_read_files.contains(*file_path))
    {
        if excerpts.len() >= POST_COMPACT_FILE_EXCERPT_MAX_FILES {
            break;
        }

        let Some(resolved) = resolve_post_compact_file_path(project_root, cwd, file_path).await
        else {
            continue;
        };
        let Some(excerpt) = read_post_compact_file_excerpt(&resolved).await else {
            continue;
        };
        let display_path = resolved
            .strip_prefix(project_root)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| file_path.clone());
        excerpts.push(format!("- Excerpt from {}: {}", display_path, excerpt));
    }
    excerpts
}

pub(super) fn collect_preserved_read_file_paths(messages: &[Message]) -> HashSet<String> {
    let tool_results = messages
        .iter()
        .filter(|message| matches!(message.role, Role::Tool))
        .filter_map(|message| message.tool_call_id.as_deref())
        .collect::<HashSet<_>>();
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

fn task_restore_activity_key(task: &RuntimeTask) -> &str {
    task.last_progress_at
        .as_deref()
        .or(task.completed_at.as_deref())
        .or(task.started_at.as_deref())
        .unwrap_or(task.created_at.as_str())
}

pub(super) fn render_task_restore_lines(mut tasks: Vec<RuntimeTask>) -> Vec<String> {
    tasks.sort_by(|a, b| task_restore_activity_key(b).cmp(task_restore_activity_key(a)));
    let mut lines = vec![POST_COMPACT_TASKS_PREFIX.to_string()];
    let relevant = tasks
        .into_iter()
        .filter(|task| {
            matches!(
                task.status,
                yode_tools::RuntimeTaskStatus::Pending
                    | yode_tools::RuntimeTaskStatus::Running
                    | yode_tools::RuntimeTaskStatus::Completed
                    | yode_tools::RuntimeTaskStatus::Failed
            )
        })
        .take(5)
        .collect::<Vec<_>>();
    if relevant.is_empty() {
        lines.push("- No async runtime tasks to restore.".to_string());
        return lines;
    }
    for task in relevant {
        lines.push(format!(
            "- Task {} [{}:{}] output={} transcript={} last_progress={} retrieval=use /tasks read {} or task_output",
            task.id,
            task.kind,
            format!("{:?}", task.status).to_ascii_lowercase(),
            task.output_path,
            task.transcript_path.as_deref().unwrap_or("none"),
            task.last_progress.as_deref().unwrap_or("none"),
            task.id
        ));
    }
    lines.push("- Restore contract: do not respawn these tasks only because compact removed earlier task context.".to_string());
    lines
}

pub(super) fn restore_block_kind_from_content(content: &str) -> Option<RestoreBlockKind> {
    if content.starts_with(POST_COMPACT_RUNTIME_PREFIX) {
        Some(RestoreBlockKind::Runtime)
    } else if content.starts_with(POST_COMPACT_FILES_PREFIX) {
        Some(RestoreBlockKind::Files)
    } else if content.starts_with(POST_COMPACT_PLAN_PREFIX) {
        Some(RestoreBlockKind::Plan)
    } else if content.starts_with(POST_COMPACT_TASKS_PREFIX) {
        Some(RestoreBlockKind::Tasks)
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

pub(super) fn restore_block_body(content: &str) -> &str {
    content
        .split_once('\n')
        .map(|(_, body)| body.trim())
        .filter(|body| !body.is_empty())
        .unwrap_or_else(|| content.trim())
}

pub(super) fn ordered_restore_block_contents(
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

fn append_restore_budget_lines(lines: &mut Vec<String>, body_lines: &[&str]) {
    lines.extend(
        body_lines
            .iter()
            .filter(|line| line.starts_with("- Restore budget:"))
            .map(|line| (*line).to_string()),
    );
}

pub(super) fn sanitize_restore_block_for_request(
    kind: RestoreBlockKind,
    content: &str,
) -> Option<String> {
    let body_lines = content
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    let mut lines = match kind {
        RestoreBlockKind::Runtime => {
            let mut lines = vec![POST_COMPACT_RUNTIME_PREFIX.to_string()];
            for &line in &body_lines {
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
            for &line in &body_lines {
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
            for &line in &body_lines {
                if line.starts_with("- Plan mode:")
                    || line.starts_with("- Permission mode:")
                    || line.starts_with("- Active plan file:")
                    || line.starts_with("- Restore contract:")
                {
                    lines.push(line.to_string());
                }
            }
            if lines.len() == 1 {
                lines.push("- Plan mode: unknown".to_string());
            }
            lines
        }
        RestoreBlockKind::Tasks => {
            let mut lines = vec![POST_COMPACT_TASKS_PREFIX.to_string()];
            for &line in &body_lines {
                if line.starts_with("- Task ")
                    || line.starts_with("- No async runtime tasks")
                    || line.starts_with("- Restore contract:")
                {
                    lines.push(line.to_string());
                }
            }
            if lines.len() == 1 {
                lines.push("- No async runtime tasks to restore.".to_string());
            }
            lines
        }
        RestoreBlockKind::Tools => vec![
            POST_COMPACT_TOOLS_PREFIX.to_string(),
            "- Tool availability follows the current runtime tool pool and permission state."
                .to_string(),
        ],
        RestoreBlockKind::PromptCache => {
            let mut lines = vec![POST_COMPACT_PROMPT_CACHE_PREFIX.to_string()];
            for &line in &body_lines {
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
            let mut in_invoked_skills = false;
            for &line in &body_lines {
                if line.starts_with("- Path-gated active skills:")
                    || line.starts_with("- Available skills:")
                    || line.starts_with("- No skills discovered.")
                {
                    lines.push(line.to_string());
                    found = true;
                    in_invoked_skills = false;
                } else if line.starts_with("- Recently invoked skills:") {
                    lines.push(line.to_string());
                    found = true;
                    in_invoked_skills = true;
                } else if in_invoked_skills
                    && (line.starts_with("- ")
                        || line.starts_with("excerpt:")
                        || line.starts_with("recovery:"))
                {
                    lines.push(line.to_string());
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

    append_restore_budget_lines(&mut lines, &body_lines);
    Some(lines.join("\n"))
}

pub(super) fn sanitized_request_restore_block_contents(
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

pub(super) fn render_hidden_post_compact_restore_prompt(
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

pub(super) async fn write_post_compact_restore_artifact_async(
    project_root: &Path,
    session_id: &str,
    mode: &str,
    blocks: &[(RestoreBlockKind, String)],
    compact_boundary: Option<&CompactBoundaryRuntimeState>,
    restore_budget: Option<&RestoreBudgetRuntimeState>,
) -> Result<Option<PathBuf>> {
    let dir = project_root.join(".yode").join("status");
    tokio::fs::create_dir_all(&dir).await.with_context(|| {
        format!(
            "failed to create status artifact directory {}",
            dir.display()
        )
    })?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-post-compact-restore.md", short_session));
    let body = render_post_compact_restore_artifact_body(
        session_id,
        mode,
        blocks,
        compact_boundary,
        restore_budget,
    )
    .context("failed to render post-compact restore artifact")?;

    tokio::fs::write(&path, body)
        .await
        .with_context(|| format!("failed to write restore artifact {}", path.display()))?;
    Ok(Some(path))
}

fn render_post_compact_restore_artifact_body(
    session_id: &str,
    mode: &str,
    blocks: &[(RestoreBlockKind, String)],
    compact_boundary: Option<&CompactBoundaryRuntimeState>,
    restore_budget: Option<&RestoreBudgetRuntimeState>,
) -> Option<String> {
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
    if let Some(budget) = restore_budget {
        body.push_str(&super::budget::render_restore_budget_table(budget));
        body.push_str("\n\n");
    }

    Some(body)
}

pub(super) async fn write_post_compact_restore_state_artifact_async(
    project_root: &Path,
    session_id: &str,
    mode: &str,
    blocks: &[(RestoreBlockKind, String)],
    compact_boundary: Option<&CompactBoundaryRuntimeState>,
    restore_budget: Option<&RestoreBudgetRuntimeState>,
) -> Result<Option<PathBuf>> {
    let dir = project_root.join(".yode").join("status");
    tokio::fs::create_dir_all(&dir).await.with_context(|| {
        format!(
            "failed to create status artifact directory {}",
            dir.display()
        )
    })?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-post-compact-restore-state.json", short_session));
    let payload = render_post_compact_restore_state_artifact_payload(
        session_id,
        mode,
        blocks,
        compact_boundary,
        restore_budget,
    );

    let body = serde_json::to_string_pretty(&payload)
        .context("failed to serialize post-compact restore state artifact")?;
    tokio::fs::write(&path, body)
        .await
        .with_context(|| format!("failed to write restore state artifact {}", path.display()))?;
    Ok(Some(path))
}

fn render_post_compact_restore_state_artifact_payload(
    session_id: &str,
    mode: &str,
    blocks: &[(RestoreBlockKind, String)],
    compact_boundary: Option<&CompactBoundaryRuntimeState>,
    restore_budget: Option<&RestoreBudgetRuntimeState>,
) -> serde_json::Value {
    let payload = serde_json::json!({
        "session_id": session_id,
        "mode": mode,
        "updated_at": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        "compact_boundary": compact_boundary,
        "restore_budget": restore_budget,
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

    payload
}

pub(super) async fn write_post_compact_restore_diff_artifact_async(
    project_root: &Path,
    session_id: &str,
    previous: &[(RestoreBlockKind, String)],
    current: &[(RestoreBlockKind, String)],
) -> Result<Option<PathBuf>> {
    if previous == current {
        return Ok(None);
    }

    let dir = project_root.join(".yode").join("status");
    tokio::fs::create_dir_all(&dir).await.with_context(|| {
        format!(
            "failed to create status artifact directory {}",
            dir.display()
        )
    })?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-post-compact-restore-diff.md", short_session));
    let body = render_post_compact_restore_diff_artifact_body(session_id, previous, current);

    tokio::fs::write(&path, body)
        .await
        .with_context(|| format!("failed to write restore diff artifact {}", path.display()))?;
    Ok(Some(path))
}

fn render_post_compact_restore_diff_artifact_body(
    session_id: &str,
    previous: &[(RestoreBlockKind, String)],
    current: &[(RestoreBlockKind, String)],
) -> String {
    let previous_map = previous
        .iter()
        .map(|(kind, content)| (*kind, content))
        .collect::<std::collections::BTreeMap<_, _>>();
    let current_map = current
        .iter()
        .map(|(kind, content)| (*kind, content))
        .collect::<std::collections::BTreeMap<_, _>>();

    let mut body = format!(
        "# Post-compact Restore Diff\n\n- Session: {}\n- Timestamp: {}\n\n",
        session_id,
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );

    for kind in [
        RestoreBlockKind::Runtime,
        RestoreBlockKind::Files,
        RestoreBlockKind::Plan,
        RestoreBlockKind::Tasks,
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

    body
}

pub(super) fn load_post_compact_restore_state_artifact(
    project_root: &Path,
    session_id: &str,
) -> Option<Vec<(RestoreBlockKind, String)>> {
    let path = post_compact_restore_state_artifact_path(project_root, session_id);
    let content = std::fs::read_to_string(path).ok()?;
    parse_post_compact_restore_state_content(&content)
}

pub(super) async fn load_post_compact_restore_state_artifact_async(
    project_root: &Path,
    session_id: &str,
) -> Option<Vec<(RestoreBlockKind, String)>> {
    let path = post_compact_restore_state_artifact_path(project_root, session_id);
    let content = tokio::fs::read_to_string(path).await.ok()?;
    parse_post_compact_restore_state_content(&content)
}

fn post_compact_restore_state_artifact_path(project_root: &Path, session_id: &str) -> PathBuf {
    let short_session = session_id.chars().take(8).collect::<String>();
    project_root
        .join(".yode")
        .join("status")
        .join(format!("{}-post-compact-restore-state.json", short_session))
}

fn parse_post_compact_restore_state_content(
    content: &str,
) -> Option<Vec<(RestoreBlockKind, String)>> {
    let value = serde_json::from_str::<serde_json::Value>(content).ok()?;
    parse_post_compact_restore_state(value)
}

fn parse_post_compact_restore_state(
    value: serde_json::Value,
) -> Option<Vec<(RestoreBlockKind, String)>> {
    let blocks = value.get("blocks")?.as_array()?;
    let mut restored = Vec::new();
    for block in blocks {
        let owned = serde_json::from_value::<OwnedRestoreBlockArtifact>(block.clone()).ok()?;
        let kind = match owned.kind.as_str() {
            "runtime" => RestoreBlockKind::Runtime,
            "files" => RestoreBlockKind::Files,
            "plan" => RestoreBlockKind::Plan,
            "tasks" => RestoreBlockKind::Tasks,
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
