pub mod commands;
pub mod completion;
mod detail_inspector;
mod engine_events;
pub mod history;
pub mod input;
mod key_dispatch;
mod key_handlers;
mod lifecycle;
pub(crate) mod rendering;
mod runtime;
mod scrollback;
mod state;
mod time;
mod turn_flow;
pub mod wizard;

use regex::Regex;
use std::sync::LazyLock;

use crate::system_message::append_grouped_system_entry;

pub use self::runtime::run;
pub(crate) use self::time::format_duration;
pub(crate) use self::state::SPINNER_VERBS;
pub(crate) use self::detail_inspector::{
    open_latest_tool_inspector, open_pending_confirmation_inspector,
};
pub use self::state::{
    App, ChatEntry, ChatRole, InspectorView, PendingConfirmation, PermissionMode, SessionState,
    ThinkingState, TurnStatus,
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

pub(crate) fn push_system_entry(app: &mut App, content: impl Into<String>) {
    append_grouped_system_entry(&mut app.chat_entries, content);
}

pub(crate) use self::scrollback::entry_formatting::{
    format_entry_as_strings as format_scrollback_entry_as_strings,
    format_grouped_system_batch as format_scrollback_grouped_system_batch,
    format_grouped_tool_batch as format_scrollback_grouped_tool_batch,
};

// ── Scrollback printing ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use yode_llm::registry::ProviderRegistry;
    use yode_tools::registry::ToolRegistry;

    use crate::system_message::append_grouped_system_entry;

    use super::{push_system_entry, App};

    fn test_app() -> App {
        App::new(
            "test-model".to_string(),
            "session-1234".to_string(),
            "/tmp".to_string(),
            "test".to_string(),
            Vec::new(),
            HashMap::new(),
            Arc::new(ProviderRegistry::new()),
            Arc::new(ToolRegistry::new()),
        )
    }

    #[test]
    fn groups_system_entries_by_semantic_title() {
        let mut app = test_app();
        append_grouped_system_entry(
            &mut app.chat_entries,
            "Context compressed · auto · -4 msgs".to_string(),
        );
        append_grouped_system_entry(
            &mut app.chat_entries,
            "Context compressed · manual · -2 msgs".to_string(),
        );
        assert_eq!(app.chat_entries.len(), 1);
        assert!(app.chat_entries[0].content.contains("auto · -4 msgs"));
        assert!(app.chat_entries[0].content.contains("manual · -2 msgs"));
    }

    #[test]
    fn keeps_unrelated_system_entries_separate() {
        let mut app = test_app();
        append_grouped_system_entry(
            &mut app.chat_entries,
            "Context compressed · auto · -4 msgs".to_string(),
        );
        append_grouped_system_entry(
            &mut app.chat_entries,
            "Session memory updated · summary · /tmp/live.md".to_string(),
        );
        assert_eq!(app.chat_entries.len(), 2);
    }

    #[test]
    fn push_system_entry_uses_semantic_title_for_grouping() {
        let mut app = test_app();
        push_system_entry(&mut app, "Session memory updated · summary · /tmp/a.md");
        push_system_entry(&mut app, "Session memory updated · snapshot · /tmp/b.md");
        assert_eq!(app.chat_entries.len(), 1);
        assert!(app.chat_entries[0].content.contains("/tmp/a.md"));
        assert!(app.chat_entries[0].content.contains("/tmp/b.md"));
    }
}
