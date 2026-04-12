use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub(crate) struct RuntimeArtifactLinks {
    pub tool: Option<String>,
    pub transcript: Option<String>,
    pub session_memory: Option<String>,
    pub recovery: Option<String>,
    pub permission: Option<String>,
}

pub(crate) fn latest_runtime_artifact_links(
    runtime: Option<yode_core::engine::EngineRuntimeState>,
) -> RuntimeArtifactLinks {
    runtime
        .map(|state| RuntimeArtifactLinks {
            tool: state.last_tool_turn_artifact_path,
            transcript: state.last_compaction_transcript_path,
            session_memory: state.last_compaction_session_memory_path,
            recovery: state.last_recovery_artifact_path,
            permission: state.last_permission_artifact_path,
        })
        .unwrap_or_default()
}

pub(crate) fn latest_artifact_candidates_from_links(
    links: &RuntimeArtifactLinks,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for maybe_path in [
        links.tool.as_deref(),
        links.transcript.as_deref(),
        links.session_memory.as_deref(),
        links.recovery.as_deref(),
        links.permission.as_deref(),
    ] {
        if let Some(path) = maybe_path {
            paths.push(PathBuf::from(path));
        }
    }
    paths
}

pub(crate) fn truncate_preview_line(text: &str, max_chars: usize) -> String {
    let squashed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if squashed.chars().count() <= max_chars {
        return squashed;
    }
    format!("{}...", squashed.chars().take(max_chars).collect::<String>())
}
