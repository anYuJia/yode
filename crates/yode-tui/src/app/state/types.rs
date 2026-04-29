use std::time::{Duration, Instant};

use crate::ui::inspector::{InspectorDocument, PanelStackCoordinator};
use tokio_util::sync::CancellationToken;

/// A pending tool confirmation.
#[derive(Debug, Clone)]
pub struct PendingConfirmation {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// Chat display entry.
#[derive(Debug, Clone)]
pub struct ChatEntry {
    pub role: ChatRole,
    pub content: String,
    pub reasoning: Option<String>,
    pub timestamp: Instant,
    pub already_printed: bool,
    pub duration: Option<Duration>,
    pub progress: Option<yode_tools::tool::ToolProgress>,
    pub tool_metadata: Option<serde_json::Value>,
    pub tool_error_type: Option<String>,
}

impl ChatEntry {
    pub fn new(role: ChatRole, content: String) -> Self {
        Self {
            role,
            content,
            reasoning: None,
            timestamp: Instant::now(),
            already_printed: false,
            duration: None,
            progress: None,
            tool_metadata: None,
            tool_error_type: None,
        }
    }

    pub fn new_with_reasoning(role: ChatRole, content: String, reasoning: Option<String>) -> Self {
        Self {
            role,
            content,
            reasoning,
            timestamp: Instant::now(),
            already_printed: false,
            duration: None,
            progress: None,
            tool_metadata: None,
            tool_error_type: None,
        }
    }
}

/// Role for display purposes.
#[derive(Debug, Clone)]
pub enum ChatRole {
    User,
    Assistant,
    ToolCall {
        id: String,
        name: String,
    },
    ToolResult {
        id: String,
        name: String,
        is_error: bool,
    },
    Error,
    System,
    SubAgentCall {
        description: String,
    },
    SubAgentToolCall {
        name: String,
    },
    SubAgentResult,
    AskUser {
        id: String,
    },
}

/// Permission mode for tool execution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PermissionMode {
    Normal,
    AutoAccept,
    Plan,
}

#[derive(Debug, Clone)]
pub struct InspectorView {
    pub document: InspectorDocument,
}

#[derive(Debug, Clone, Default)]
pub struct InspectorRuntime {
    pub stack: PanelStackCoordinator,
    pub views: Vec<InspectorView>,
}

impl PermissionMode {
    pub fn label(&self) -> &'static str {
        match self {
            PermissionMode::Normal => "Default",
            PermissionMode::AutoAccept => "Auto",
            PermissionMode::Plan => "Plan",
        }
    }

    pub fn next(self) -> Self {
        match self {
            PermissionMode::Normal => PermissionMode::AutoAccept,
            PermissionMode::AutoAccept => PermissionMode::Plan,
            PermissionMode::Plan => PermissionMode::Normal,
        }
    }
}

/// Persistent session state (model, tokens, etc.)
pub struct SessionState {
    pub model: String,
    pub session_id: String,
    pub working_dir: String,
    pub startup_profile: Option<String>,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    pub previous_prompt_tokens: u32,
    pub tool_call_count: u32,
    pub permission_mode: PermissionMode,
    pub always_allow_tools: Vec<String>,
    pub input_estimated: bool,
    pub turn_input_tokens: u32,
    pub turn_output_tokens: u32,
    pub(crate) resume_cache_warmup: Option<crate::commands::info::ResumeTranscriptCacheWarmupStats>,
}

/// Unified status: Idle -> Working -> Done (or Retrying -> Working -> Done)
#[derive(Debug, Clone)]
pub enum TurnStatus {
    Idle,
    Working {
        verb: &'static str,
    },
    Done {
        elapsed: Duration,
        tools: u32,
    },
    Retrying {
        verb: &'static str,
        error: String,
        attempt: u32,
        max_attempts: u32,
        delay_secs: u64,
    },
}

impl TurnStatus {
    pub fn is_visible(&self) -> bool {
        !matches!(self, TurnStatus::Idle)
    }
}

const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub(crate) const SPINNER_VERBS: &[&str] = &[
    "Thinking",
    "Analyzing",
    "Reasoning",
    "Planning",
    "Inspecting",
    "Reviewing",
    "Checking",
    "Tracing",
    "Comparing",
    "Exploring",
    "Reading",
    "Searching",
    "Drafting",
    "Refining",
    "Synthesizing",
    "Organizing",
    "Ruminating",
    "Cogitating",
    "Orchestrating",
    "Assembling",
    "Coordinating",
    "Verifying",
    "Validating",
    "Summarizing",
    "Clarifying",
    "Resolving",
];

/// Spinner advances every SPINNER_TICK_DIVISOR ticks.
const SPINNER_TICK_DIVISOR: usize = 4;

pub struct ThinkingState {
    pub active: bool,
    pub spinner_frame: usize,
    pub started_at: Option<Instant>,
    pub cancel_token: Option<CancellationToken>,
    tick_count: usize,
}

impl ThinkingState {
    pub fn new() -> Self {
        Self {
            active: false,
            spinner_frame: 0,
            started_at: None,
            cancel_token: None,
            tick_count: 0,
        }
    }

    pub fn start(&mut self, token: CancellationToken) {
        self.active = true;
        self.started_at = Some(Instant::now());
        self.cancel_token = Some(token);
    }

    pub fn stop(&mut self) {
        self.active = false;
        self.started_at = None;
        self.cancel_token = None;
    }

    pub fn cancel(&mut self) {
        if let Some(token) = self.cancel_token.take() {
            token.cancel();
        }
        self.stop();
    }

    pub fn spinner_char(&self) -> char {
        SPINNER_FRAMES[self.spinner_frame % SPINNER_FRAMES.len()]
    }

    pub fn elapsed_secs(&self) -> u64 {
        self.started_at
            .map(|started_at| started_at.elapsed().as_secs())
            .unwrap_or(0)
    }

    pub fn advance_spinner(&mut self) -> bool {
        self.tick_count += 1;
        if self.tick_count >= SPINNER_TICK_DIVISOR {
            self.tick_count = 0;
            self.spinner_frame = (self.spinner_frame + 1) % SPINNER_FRAMES.len();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PermissionMode, ThinkingState};
    use tokio_util::sync::CancellationToken;

    #[test]
    fn permission_mode_cycles_through_operator_modes() {
        assert_eq!(PermissionMode::Normal.label(), "Default");
        assert_eq!(PermissionMode::Normal.next(), PermissionMode::AutoAccept);
        assert_eq!(PermissionMode::AutoAccept.next(), PermissionMode::Plan);
        assert_eq!(PermissionMode::Plan.next(), PermissionMode::Normal);
    }

    #[test]
    fn thinking_state_tracks_spinner_and_cancellation() {
        let token = CancellationToken::new();
        let mut thinking = ThinkingState::new();

        thinking.start(token.clone());
        assert!(thinking.active);
        assert_eq!(thinking.elapsed_secs(), 0);
        assert_eq!(thinking.spinner_char(), '⠋');
        assert!(!thinking.advance_spinner());
        assert!(!thinking.advance_spinner());
        assert!(!thinking.advance_spinner());
        assert!(thinking.advance_spinner());
        assert_eq!(thinking.spinner_char(), '⠙');

        thinking.cancel();
        assert!(token.is_cancelled());
        assert!(!thinking.active);
        assert!(thinking.cancel_token.is_none());
    }

    #[test]
    fn advance_spinner_only_reports_true_on_visible_frame_change() {
        let mut state = ThinkingState::new();
        assert!(!state.advance_spinner());
        assert!(!state.advance_spinner());
        assert!(!state.advance_spinner());
        assert!(state.advance_spinner());
    }
}
