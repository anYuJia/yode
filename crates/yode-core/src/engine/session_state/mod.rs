mod artifacts;
mod memory;

use super::*;

impl AgentEngine {
    /// Get the current message history.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Get the context.
    pub fn context(&self) -> &AgentContext {
        &self.context
    }

    /// Restore messages from database for a resumed session.
    pub fn restore_messages(&mut self, messages: Vec<Message>) {
        self.messages.clear();
        self.failed_tool_call_ids.clear();
        self.clear_cache_edit_tracking();
        self.post_compact_restore_blocks.clear();
        self.messages
            .push(Message::system(self.system_prompt.clone()));
        self.messages.extend(messages);
        self.rehydrate_post_compact_restore_messages();
        self.set_expected_prompt_cache_drop_reason("restore_messages");
        self.reset_autocompact_state();
        self.compaction_cause_histogram.clear();
        self.rebuild_runtime_artifact_state_from_disk();
        info!(
            "Restored {} messages from database",
            self.messages.len() - 1
        );
    }

    pub fn restore_and_persist_messages(&mut self, messages: Vec<Message>) {
        self.restore_messages(messages);
        self.sync_persisted_messages_snapshot();
        self.persist_session_artifacts();
    }

    /// Clear conversation history, keeping only the system prompt.
    pub fn clear_conversation(&mut self) {
        if self.messages.len() > 1 {
            self.messages.clear();
            self.failed_tool_call_ids.clear();
            self.messages
                .push(Message::system(self.system_prompt.clone()));
            info!("Cleared conversation, kept system prompt");
        }
        self.clear_cache_edit_tracking();
        self.post_compact_restore_blocks.clear();
        if let Err(err) = clear_live_session_memory(&self.context.working_dir_compat()) {
            warn!(
                "Failed to clear live session memory during conversation reset: {}",
                err
            );
        }
        self.reset_live_session_memory_tracking();
        self.last_compaction_mode = None;
        self.last_compaction_at = None;
        self.last_compaction_summary_excerpt = None;
        self.last_compaction_session_memory_path = None;
        self.last_compaction_transcript_path = None;
        self.last_compact_boundary = None;
        self.last_restore_budget = None;
        self.total_compactions = 0;
        self.auto_compactions = 0;
        self.manual_compactions = 0;
        self.compaction_cause_histogram.clear();
        self.set_shared_memory_status(None, None, false, 0);
        self.sync_persisted_messages_snapshot();
        self.rebuild_system_prompt();
        self.set_expected_prompt_cache_drop_reason("clear_conversation");
        self.reset_autocompact_state();
    }
}
