pub mod commands;
pub mod completion;
mod engine_events;
pub mod history;
pub mod input;
mod key_dispatch;
mod key_handlers;
mod lifecycle;
mod rendering;
mod runtime;
mod scrollback;
mod state;
mod turn_flow;
pub mod wizard;

use regex::Regex;
use std::sync::LazyLock;
use std::time::Duration;

pub use self::runtime::run;
pub(crate) use self::scrollback::format_duration;
pub(crate) use self::state::SPINNER_VERBS;
pub use self::state::{
    App, ChatEntry, ChatRole, PendingConfirmation, PermissionMode, SessionState, ThinkingState,
    TurnStatus,
};

// ── Content Filtering ───────────────────────────────────────────────

static TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Catch everything from standard tags to malformed snippets and partial results
    Regex::new(r"(?s)\[DUMMY_TOOL_RESULT\]?|\[tool_use\s+[^\]>]+[\]>](?:\s*[:]\s*)?\{.*?\}[\s\]>]*|\[tool_result\s+[^\]>]+[\]>](?:\s*[:]\s*)?\{.*?\}[\s\]>]*|\[tool_(?:use|result)\s+[^\]>]+[\]>]?").unwrap()
});

/// Strips internal protocol tags from assistant text output.
fn strip_internal_tags(text: &str) -> String {
    TAG_RE.replace_all(text, "").to_string()
}

// ── Skill Command Wrapper ──────────────────────────────────────────

/// Dynamic skill command wrapper that delegates execution via the engine.
struct SkillCommandWrapper {
    meta: crate::commands::CommandMeta,
}

impl crate::commands::Command for SkillCommandWrapper {
    fn meta(&self) -> &crate::commands::CommandMeta {
        &self.meta
    }

    fn execute(
        &self,
        _args: &str,
        _ctx: &mut crate::commands::context::CommandContext,
    ) -> crate::commands::CommandResult {
        // Skill commands are handled by showing the skill description;
        // actual execution flows through the normal chat/engine path.
        Ok(crate::commands::CommandOutput::Message(format!(
            "Skill command: {}",
            self.meta.description
        )))
    }
}

// SAFETY: SkillCommandWrapper holds only static references and is safe to share.
unsafe impl Send for SkillCommandWrapper {}
unsafe impl Sync for SkillCommandWrapper {}

/// Find substring case-insensitively, return byte offset
fn find_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    haystack.to_lowercase().find(&needle.to_lowercase())
}

fn push_grouped_system_entry(app: &mut App, group_prefix: &str, content: String) {
    if let Some(last) = app.chat_entries.last_mut() {
        if matches!(last.role, ChatRole::System)
            && last.content.starts_with(group_prefix)
            && last.timestamp.elapsed() <= Duration::from_secs(5)
        {
            if !last.content.contains(&content) {
                last.content.push('\n');
                last.content.push_str(&content);
            }
            return;
        }
    }
    app.chat_entries
        .push(ChatEntry::new(ChatRole::System, content));
}

// ── Scrollback printing ─────────────────────────────────────────────
