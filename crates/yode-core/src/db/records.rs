use chrono::{DateTime, Utc};

use crate::session::Session;

/// A stored message in a session.
#[derive(Debug, Clone)]
pub struct StoredMessage {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub content: Option<String>,
    pub reasoning: Option<String>,
    pub tool_calls_json: Option<String>,
    pub tool_call_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionArtifacts {
    pub last_compaction_mode: Option<String>,
    pub last_compaction_at: Option<String>,
    pub last_compaction_summary_excerpt: Option<String>,
    pub last_compaction_session_memory_path: Option<String>,
    pub last_compaction_transcript_path: Option<String>,
    pub last_session_memory_update_at: Option<String>,
    pub last_session_memory_update_path: Option<String>,
    pub last_session_memory_generated_summary: bool,
}

#[derive(Debug, Clone)]
pub struct SessionListEntry {
    pub session: Session,
    pub artifacts: SessionArtifacts,
}
