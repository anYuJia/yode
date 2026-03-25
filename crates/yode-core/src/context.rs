use std::path::PathBuf;
use uuid::Uuid;

/// Runtime context for an agent session.
#[derive(Debug, Clone)]
pub struct AgentContext {
    /// Unique session identifier
    pub session_id: String,
    /// Current working directory
    pub working_dir: PathBuf,
    /// Model being used
    pub model: String,
    /// Provider name
    pub provider: String,
    /// Whether this session was resumed from a previous one
    pub is_resumed: bool,
}

impl AgentContext {
    pub fn new(working_dir: PathBuf, provider: String, model: String) -> Self {
        Self {
            session_id: Uuid::new_v4().to_string(),
            working_dir,
            model,
            provider,
            is_resumed: false,
        }
    }

    /// Create a context that resumes an existing session.
    pub fn resume(session_id: String, working_dir: PathBuf, provider: String, model: String) -> Self {
        Self {
            session_id,
            working_dir,
            model,
            provider,
            is_resumed: true,
        }
    }
}
