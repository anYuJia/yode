use std::path::PathBuf;

use crate::commands::artifact_nav::{
    latest_artifact_by_suffix, latest_post_compact_restore_artifact,
    latest_post_compact_restore_state_artifact, latest_prompt_cache_artifact,
    latest_prompt_cache_break_artifact, latest_prompt_cache_diff_artifact,
    latest_prompt_cache_events_artifact, latest_prompt_cache_state_artifact,
};

#[derive(Debug, Clone, Default)]
pub(crate) struct RuntimeArtifactLinks {
    pub tool: Option<String>,
    pub transcript: Option<String>,
    pub session_memory: Option<String>,
    pub recovery: Option<String>,
    pub permission: Option<String>,
    pub prompt_cache: Option<String>,
    pub prompt_cache_events: Option<String>,
    pub prompt_cache_break: Option<String>,
    pub prompt_cache_diff: Option<String>,
    pub prompt_cache_state: Option<String>,
    pub post_compact_restore: Option<String>,
    pub post_compact_restore_state: Option<String>,
}

pub(crate) fn latest_runtime_artifact_links(
    project_root: &std::path::Path,
    runtime: Option<yode_core::engine::EngineRuntimeState>,
) -> RuntimeArtifactLinks {
    runtime
        .map(|state| RuntimeArtifactLinks {
            tool: state.last_tool_turn_artifact_path,
            transcript: state.last_compaction_transcript_path,
            session_memory: state.last_compaction_session_memory_path,
            recovery: state.last_recovery_artifact_path,
            permission: state.last_permission_artifact_path,
            prompt_cache: latest_prompt_cache_artifact(project_root)
                .map(|path| path.display().to_string()),
            prompt_cache_events: latest_prompt_cache_events_artifact(project_root)
                .map(|path| path.display().to_string()),
            prompt_cache_break: latest_prompt_cache_break_artifact(project_root)
                .map(|path| path.display().to_string()),
            prompt_cache_diff: latest_prompt_cache_diff_artifact(project_root)
                .map(|path| path.display().to_string()),
            prompt_cache_state: latest_prompt_cache_state_artifact(project_root)
                .map(|path| path.display().to_string()),
            post_compact_restore: latest_post_compact_restore_artifact(project_root)
                .map(|path| path.display().to_string()),
            post_compact_restore_state: latest_post_compact_restore_state_artifact(project_root)
                .map(|path| path.display().to_string()),
        })
        .unwrap_or_default()
}

pub(crate) fn latest_artifact_candidates_from_links(links: &RuntimeArtifactLinks) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for maybe_path in [
        links.tool.as_deref(),
        links.transcript.as_deref(),
        links.session_memory.as_deref(),
        links.recovery.as_deref(),
        links.permission.as_deref(),
        links.prompt_cache.as_deref(),
        links.prompt_cache_events.as_deref(),
        links.prompt_cache_break.as_deref(),
        links.prompt_cache_diff.as_deref(),
        links.prompt_cache_state.as_deref(),
        links.post_compact_restore.as_deref(),
        links.post_compact_restore_state.as_deref(),
    ] {
        if let Some(path) = maybe_path {
            paths.push(PathBuf::from(path));
        }
    }
    paths
}

pub(crate) fn dedup_artifact_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = std::collections::BTreeSet::new();
    let mut deduped = Vec::new();
    for path in paths {
        let key = path.display().to_string();
        if seen.insert(key) {
            deduped.push(path);
        }
    }
    deduped
}

pub(crate) fn startup_artifact_candidates(project_root: &std::path::Path) -> Vec<PathBuf> {
    let dir = project_root.join(".yode").join("startup");
    [
        "startup-profile.txt",
        "resume-warmup.json",
        "provider-inventory.json",
        "startup-bundle-manifest.json",
        "mcp-startup-failures.json",
        "settings-scopes.json",
        "managed-mcp-inventory.json",
        "tool-search-activation.json",
    ]
    .into_iter()
    .filter_map(|suffix| latest_artifact_by_suffix(&dir, suffix))
    .collect()
}

pub(crate) fn doctor_bundle_references(cwd: &std::path::Path) -> Vec<PathBuf> {
    std::fs::read_dir(cwd)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.starts_with("doctor-bundle-"))
                    .unwrap_or(false)
        })
        .collect()
}

pub(crate) fn truncate_preview_line(text: &str, max_chars: usize) -> String {
    let squashed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if squashed.chars().count() <= max_chars {
        return squashed;
    }
    format!(
        "{}...",
        squashed.chars().take(max_chars).collect::<String>()
    )
}
