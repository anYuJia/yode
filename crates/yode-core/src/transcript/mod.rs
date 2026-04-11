mod render;
#[cfg(test)]
mod tests;
mod writer;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use yode_llm::types::Message;

use crate::context_manager::CompressionReport;

const TRANSCRIPTS_DIR: &str = ".yode/transcripts";

pub fn write_compaction_transcript(
    project_root: &Path,
    session_id: &str,
    messages: &[Message],
    report: &CompressionReport,
    mode: &str,
    failed_tool_call_ids: &HashSet<String>,
    session_memory_path: Option<&Path>,
    files_read: &HashMap<String, usize>,
    files_modified: &[String],
) -> Result<PathBuf> {
    let dir = project_root.join(TRANSCRIPTS_DIR);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create transcript dir: {}", dir.display()))?;

    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!(
        "{}-compact-{}.md",
        render::short_session_id(session_id),
        timestamp
    ));
    writer::write_string_with_retry(
        &path,
        &render::render_compaction_transcript(
            project_root,
            session_id,
            messages,
            report,
            mode,
            failed_tool_call_ids,
            session_memory_path,
            files_read,
            files_modified,
        ),
    )
    .with_context(|| format!("Failed to write transcript file: {}", path.display()))?;

    Ok(path)
}
