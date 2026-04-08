use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Controlled session runtime state (on-the-fly updates).
#[derive(Debug)]
pub struct SessionRuntime {
    /// Current working directory, can be changed via tools like bash (cd).
    pub cwd: PathBuf,
    /// Immutable root of the project/workspace.
    pub project_root: PathBuf,
    /// Last known good directory to fall back to.
    pub last_success_cwd: PathBuf,
}

impl SessionRuntime {
    pub fn new(root: PathBuf) -> Self {
        Self {
            cwd: root.clone(),
            project_root: root.clone(),
            last_success_cwd: root,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffortLevel {
    Min,
    Low,
    Medium,
    High,
    Max,
}

impl std::fmt::Display for EffortLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Min => write!(f, "min"),
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
            Self::Max => write!(f, "max"),
        }
    }
}

impl std::str::FromStr for EffortLevel {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "min" => Ok(Self::Min),
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "max" => Ok(Self::Max),
            _ => Err(format!(
                "Unknown effort level: {s}. Valid: min, low, medium, high, max"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuerySource {
    User,
    SubAgent,
    Cron,
    Hook(String),
}

impl Default for QuerySource {
    fn default() -> Self {
        Self::User
    }
}

/// Runtime context for an agent session.
#[derive(Debug, Clone)]
pub struct AgentContext {
    /// Unique session identifier
    pub session_id: String,
    /// Managed session runtime state
    pub runtime: Arc<Mutex<SessionRuntime>>,
    /// Model being used
    pub model: String,
    /// Provider name
    pub provider: String,
    /// Whether this session was resumed from a previous one
    pub is_resumed: bool,
    /// Effort level for the session
    pub effort: EffortLevel,
    /// Output style: "default", "explanatory", "learning"
    pub output_style: String,
}

impl AgentContext {
    pub fn new(working_dir: PathBuf, provider: String, model: String) -> Self {
        Self {
            session_id: Uuid::new_v4().to_string(),
            runtime: Arc::new(Mutex::new(SessionRuntime::new(working_dir))),
            model,
            provider,
            is_resumed: false,
            effort: EffortLevel::Medium,
            output_style: "default".to_string(),
        }
    }

    /// 根据 EffortLevel 获取最大输出 Token 预算
    pub fn get_max_tokens(&self) -> u32 {
        match self.effort {
            EffortLevel::Min => 1024,
            EffortLevel::Low => 2048,
            EffortLevel::Medium => 4096,
            EffortLevel::High => 8192,
            EffortLevel::Max => 16384,
        }
    }

    /// Create a context that resumes an existing session.
    pub fn resume(
        session_id: String,
        working_dir: PathBuf,
        provider: String,
        model: String,
    ) -> Self {
        Self {
            session_id,
            runtime: Arc::new(Mutex::new(SessionRuntime::new(working_dir))),
            model,
            provider,
            is_resumed: true,
            effort: EffortLevel::Medium,
            output_style: "default".to_string(),
        }
    }

    /// Synchronously get the root directory (only for initialization or compat).
    pub fn working_dir_compat(&self) -> PathBuf {
        futures::executor::block_on(async { self.runtime.lock().await.project_root.clone() })
    }
}
