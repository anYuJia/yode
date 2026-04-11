mod io;
mod schema;
mod snapshot;

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use yode_llm::types::{Message, Role};

use crate::context_manager::CompressionReport;

const SESSION_MEMORY_RELATIVE_PATH: &str = ".yode/memory/session.md";
const LIVE_SESSION_MEMORY_RELATIVE_PATH: &str = ".yode/memory/session.live.md";
const SESSION_MEMORY_HEADER: &str = "# Session Memory\n\nYode writes this file automatically after context compaction. Newer entries appear first.";
const LIVE_SESSION_MEMORY_HEADER: &str =
    "# Session Snapshot\n\nYode refreshes this file during the session to preserve recent context between compactions.";
const MAX_SESSION_MEMORY_CHARS: usize = 16_000;
const MAX_LISTED_FILES: usize = 8;
const MEMORY_WRITE_RETRIES: usize = 3;

#[derive(Debug, Clone)]
pub struct LiveSessionSnapshot {
    pub session_id: String,
    pub total_tool_calls: u32,
    pub message_count: usize,
    pub goals: Vec<String>,
    pub findings: Vec<String>,
    pub decisions: Vec<String>,
    pub open_questions: Vec<String>,
    pub files_read: Vec<String>,
    pub files_modified: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(in crate::session_memory) struct StructuredMemorySections {
    goals: Vec<String>,
    findings: Vec<String>,
    decisions: Vec<String>,
    open_questions: Vec<String>,
}

#[derive(Debug, Clone)]
pub(in crate::session_memory) struct MemorySchemaHints {
    freshness: Vec<String>,
    confidence: Vec<String>,
}

pub use self::io::{
    clear_live_session_memory, live_session_memory_path, persist_compaction_memory,
    persist_live_session_memory, persist_live_session_memory_summary, session_memory_path,
};
pub use self::snapshot::{build_live_snapshot, render_live_session_memory_prompt};

#[cfg(test)]
mod tests;
