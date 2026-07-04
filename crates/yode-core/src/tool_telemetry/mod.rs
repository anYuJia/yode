mod helpers;

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use tokio::sync::mpsc;

use yode_llm::types::ToolCall;
use yode_tools::tool::ToolResult;

#[cfg(test)]
use crate::tool_runtime::write_tool_turn_artifact;
use crate::tool_runtime::{write_tool_turn_artifact_async, ToolPoolArtifactView, ToolTurnArtifact};

use super::{
    AgentEngine, EngineEvent, ToolExecutionTrace, TOOL_BUDGET_NOTICE, TOOL_BUDGET_WARNING,
};
use helpers::{
    extract_diff_preview, failure_signature, output_preview, summarize_result_metadata,
    tool_truncation_from_metadata,
};

fn hash_string(value: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[expect(
    clippy::too_many_arguments,
    reason = "prompt cache comparison intentionally compares four previous/current cache prefixes"
)]
fn prompt_cache_change_summary(
    previous_system: Option<&str>,
    previous_restore: Option<&str>,
    previous_tools: Option<&str>,
    previous_messages: Option<&str>,
    current_system: Option<&str>,
    current_restore: Option<&str>,
    current_tools: Option<&str>,
    current_messages: Option<&str>,
) -> String {
    let mut changed = Vec::new();
    if previous_system.is_some() && previous_system != current_system {
        changed.push("system");
    }
    if previous_restore.is_some() && previous_restore != current_restore {
        changed.push("restore");
    }
    if previous_tools.is_some() && previous_tools != current_tools {
        changed.push("tools");
    }
    if previous_messages.is_some() && previous_messages != current_messages {
        changed.push("messages");
    }

    if changed.is_empty() {
        "stable".to_string()
    } else {
        changed.join(",")
    }
}

fn prompt_cache_transition_kind_for_change_summary(change_summary: &str) -> &'static str {
    match change_summary {
        "system" => "system_prefix_changed",
        "restore" => "restore_prefix_changed",
        "tools" => "tool_prefix_changed",
        "messages" => "message_prefix_changed",
        _ => "prefix_changed",
    }
}

fn truncate_cache_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let mut truncated = text.chars().take(max_chars).collect::<String>();
        truncated.push_str("...");
        truncated
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "prompt cache diff artifact records all previous/current hashes and excerpts in one snapshot"
)]
async fn write_prompt_cache_diff_artifact_async(
    project_root: &std::path::Path,
    session_id: &str,
    transition_kind: &str,
    transition_reason: Option<&str>,
    previous_system_hash: Option<&str>,
    previous_restore_hash: Option<&str>,
    previous_tool_hash: Option<&str>,
    previous_message_hash: Option<&str>,
    current_system_hash: Option<&str>,
    current_restore_hash: Option<&str>,
    current_tool_hash: Option<&str>,
    current_message_hash: Option<&str>,
    previous_system_text: Option<&str>,
    previous_restore_text: Option<&str>,
    previous_tool_text: Option<&str>,
    previous_message_text: Option<&str>,
    current_system_text: Option<&str>,
    current_restore_text: Option<&str>,
    current_tool_text: Option<&str>,
    current_message_text: Option<&str>,
) -> Result<(String, String)> {
    let dir = project_root.join(".yode").join("status");
    tokio::fs::create_dir_all(&dir).await.with_context(|| {
        format!(
            "failed to create prompt-cache diff artifact directory {}",
            dir.display()
        )
    })?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-prompt-cache-diff.md", short_session));

    let body = format!(
        "# Prompt Cache Diff\n\n- Session: {}\n- Transition: {}\n- Reason: {}\n- Timestamp: {}\n\n## Hashes\n\n- Previous system/restore/tool/message: {} / {} / {} / {}\n- Current system/restore/tool/message: {} / {} / {} / {}\n\n## Previous System\n\n```text\n{}\n```\n\n## Current System\n\n```text\n{}\n```\n\n## Previous Restore\n\n```text\n{}\n```\n\n## Current Restore\n\n```text\n{}\n```\n\n## Previous Tools\n\n```text\n{}\n```\n\n## Current Tools\n\n```text\n{}\n```\n\n## Previous Messages\n\n```text\n{}\n```\n\n## Current Messages\n\n```text\n{}\n```\n",
        session_id,
        transition_kind,
        transition_reason.unwrap_or("none"),
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        previous_system_hash.unwrap_or("none"),
        previous_restore_hash.unwrap_or("none"),
        previous_tool_hash.unwrap_or("none"),
        previous_message_hash.unwrap_or("none"),
        current_system_hash.unwrap_or("none"),
        current_restore_hash.unwrap_or("none"),
        current_tool_hash.unwrap_or("none"),
        current_message_hash.unwrap_or("none"),
        truncate_cache_text(previous_system_text.unwrap_or("none"), 4_000),
        truncate_cache_text(current_system_text.unwrap_or("none"), 4_000),
        truncate_cache_text(previous_restore_text.unwrap_or("none"), 4_000),
        truncate_cache_text(current_restore_text.unwrap_or("none"), 4_000),
        truncate_cache_text(previous_tool_text.unwrap_or("none"), 4_000),
        truncate_cache_text(current_tool_text.unwrap_or("none"), 4_000),
        truncate_cache_text(previous_message_text.unwrap_or("none"), 4_000),
        truncate_cache_text(current_message_text.unwrap_or("none"), 4_000),
    );

    tokio::fs::write(&path, body).await.with_context(|| {
        format!(
            "failed to write prompt-cache diff artifact {}",
            path.display()
        )
    })?;
    Ok((
        path.display().to_string(),
        format!(
            "{} / {}->{} / {}",
            transition_kind,
            previous_system_hash.unwrap_or("none"),
            current_system_hash.unwrap_or("none"),
            transition_reason.unwrap_or("none")
        ),
    ))
}

impl AgentEngine {
    pub(super) fn reset_tool_turn_runtime(&mut self) {
        self.tool_turn_counter = self.tool_turn_counter.saturating_add(1);
        self.current_tool_turn_started_at = Some(Self::now_timestamp());
        self.current_turn_tool_progress_events = 0;
        self.current_turn_parallel_batches = 0;
        self.current_turn_parallel_calls = 0;
        self.current_turn_max_parallel_batch_size = 0;
        self.current_turn_budget_notice_emitted = false;
        self.current_turn_budget_warning_emitted = false;
        self.current_turn_truncated_results = 0;
        self.current_tool_execution_traces.clear();
        self.total_tool_results_bytes = 0;
        self.tool_call_count = 0;
    }

    pub(super) fn reset_prompt_cache_turn_runtime(&mut self) {
        self.prompt_cache_runtime.last_turn_prompt_tokens = None;
        self.prompt_cache_runtime.last_turn_completion_tokens = None;
        self.prompt_cache_runtime.last_turn_cache_write_tokens = None;
        self.prompt_cache_runtime.last_turn_cache_read_tokens = None;
        self.prompt_cache_runtime.last_turn_cache_edit_deletions = None;
        self.prompt_cache_runtime.last_turn_cache_deleted_tokens = None;
        let (pending, pinned) = self.active_cache_edit_refs();
        self.prompt_cache_runtime.pending_cache_edit_refs = pending.len() as u32;
        self.prompt_cache_runtime.pinned_cache_edit_refs = pinned.len() as u32;
    }

    pub(super) fn record_compaction_cause(&mut self, cause: &str) {
        *self
            .compaction_cause_histogram
            .entry(cause.to_string())
            .or_insert(0) += 1;
    }

    pub(super) async fn record_response_usage(
        &mut self,
        usage: &yode_llm::types::Usage,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) {
        self.cost_tracker.record_usage(
            usage.uncached_prompt_tokens() as u64,
            usage.completion_tokens as u64,
        );
        if usage.cache_write_tokens > 0 || usage.cache_read_tokens > 0 {
            self.cost_tracker.record_cache_usage(
                usage.cache_write_tokens as u64,
                usage.cache_read_tokens as u64,
            );
        }

        if usage.has_reported_tokens() {
            let previous_cache_read = self.prompt_cache_runtime.last_turn_cache_read_tokens;
            self.promote_pending_cache_edit_refs();
            self.prompt_cache_runtime.last_turn_prompt_tokens = Some(usage.prompt_tokens);
            self.prompt_cache_runtime.last_turn_completion_tokens = Some(usage.completion_tokens);
            self.prompt_cache_runtime.last_turn_cache_write_tokens = Some(usage.cache_write_tokens);
            self.prompt_cache_runtime.last_turn_cache_read_tokens = Some(usage.cache_read_tokens);
            self.prompt_cache_runtime.last_turn_cache_deleted_tokens =
                Some(usage.cache_deleted_tokens);
            self.detect_prompt_cache_break(previous_cache_read, usage.cache_read_tokens)
                .await;
            self.prompt_cache_runtime.reported_turns =
                self.prompt_cache_runtime.reported_turns.saturating_add(1);
            if usage.cache_write_tokens > 0 {
                self.prompt_cache_runtime.cache_write_turns = self
                    .prompt_cache_runtime
                    .cache_write_turns
                    .saturating_add(1);
            }
            if usage.cache_read_tokens > 0 {
                self.prompt_cache_runtime.cache_read_turns =
                    self.prompt_cache_runtime.cache_read_turns.saturating_add(1);
            }
            self.prompt_cache_runtime.cache_write_tokens_total = self
                .prompt_cache_runtime
                .cache_write_tokens_total
                .saturating_add(usage.cache_write_tokens as u64);
            self.prompt_cache_runtime.cache_read_tokens_total = self
                .prompt_cache_runtime
                .cache_read_tokens_total
                .saturating_add(usage.cache_read_tokens as u64);
            self.prompt_cache_runtime.cache_deleted_tokens_total = self
                .prompt_cache_runtime
                .cache_deleted_tokens_total
                .saturating_add(usage.cache_deleted_tokens as u64);
            let (pending, pinned) = self.active_cache_edit_refs();
            self.prompt_cache_runtime.pending_cache_edit_refs = pending.len() as u32;
            self.prompt_cache_runtime.pinned_cache_edit_refs = pinned.len() as u32;
        }

        let _ = event_tx.send(EngineEvent::CostUpdate {
            estimated_cost: self.cost_tracker.estimated_cost(),
            input_tokens: self.cost_tracker.usage().input_tokens,
            output_tokens: self.cost_tracker.usage().output_tokens,
            cache_write_tokens: self.cost_tracker.usage().cache_write_tokens,
            cache_read_tokens: self.cost_tracker.usage().cache_read_tokens,
        });

        if self.cost_tracker.is_over_budget() {
            let _ = event_tx.send(EngineEvent::BudgetExceeded {
                cost: self.cost_tracker.estimated_cost(),
                limit: self.cost_tracker.remaining_budget().unwrap_or(0.0),
            });
        }
    }

    pub(super) fn promote_pending_cache_edit_refs(&mut self) {
        if self.pending_cache_edit_refs.is_empty() {
            return;
        }

        for cache_ref in self.pending_cache_edit_refs.drain(..) {
            if !self.pinned_cache_edit_refs.contains(&cache_ref) {
                self.pinned_cache_edit_refs.push(cache_ref);
            }
        }
        self.pinned_cache_edit_refs.sort();
        self.pinned_cache_edit_refs.dedup();
    }

    pub(super) fn set_expected_prompt_cache_drop_reason(&mut self, reason: impl Into<String>) {
        let reason = reason.into();
        self.forced_prompt_cache_expected_drop_reason = Some(reason.clone());
        self.prompt_cache_runtime
            .last_prompt_cache_expected_drop_reason = Some(reason);
    }

    pub(super) fn clear_expected_prompt_cache_drop_reason(&mut self) {
        self.forced_prompt_cache_expected_drop_reason = None;
        self.prompt_cache_runtime
            .last_prompt_cache_expected_drop_reason = None;
    }

    pub(super) fn record_prompt_cache_request_state(
        &mut self,
        request: &yode_llm::types::ChatRequest,
    ) {
        let mut system_prefix = String::new();
        let mut restore_prefix = String::new();
        let mut tool_prefix = String::new();
        let mut message_prefix = String::new();
        message_prefix.push_str(&request.model);
        message_prefix.push('\n');

        for tool in &request.tools {
            tool_prefix.push_str(&tool.name);
            tool_prefix.push('|');
            tool_prefix.push_str(&tool.description);
            tool_prefix.push('|');
            tool_prefix.push_str(&tool.parameters.to_string());
            tool_prefix.push('\n');
            if tool_prefix.len() > 16_000 {
                break;
            }
        }

        for message in &request.messages {
            let target = if matches!(message.role, yode_llm::types::Role::System) {
                &mut system_prefix
            } else {
                &mut message_prefix
            };
            target.push_str(match message.role {
                yode_llm::types::Role::System => "system:",
                yode_llm::types::Role::User => "user:",
                yode_llm::types::Role::Assistant => "assistant:",
                yode_llm::types::Role::Tool => "tool:",
            });
            if let Some(content) = message.content.as_deref() {
                let excerpt = content.chars().take(512).collect::<String>();
                target.push_str(&excerpt);
            }
            for tool_call in &message.tool_calls {
                target.push_str(&tool_call.id);
                target.push_str(&tool_call.name);
            }
            target.push('\n');
            if target.len() > 16_000 {
                break;
            }
        }

        for block in &request.provider_hints.restore_system_blocks {
            restore_prefix.push_str("system:");
            let excerpt = format!("[Post-compact restore: {}]\n{}", block.kind, block.content)
                .chars()
                .take(512)
                .collect::<String>();
            restore_prefix.push_str(&excerpt);
            restore_prefix.push('\n');
            if restore_prefix.len() > 16_000 {
                break;
            }
        }

        if let Some(hints) = request.provider_hints.anthropic.as_ref() {
            message_prefix.push_str(&format!(
                "anthropic_cache={};pending={:?};pinned={:?}",
                hints.enable_prompt_caching,
                hints.pending_deleted_cache_references,
                hints.pinned_deleted_cache_references
            ));
        }

        self.pending_prompt_cache_system_hash = Some(hash_string(&system_prefix));
        self.pending_prompt_cache_restore_hash = Some(hash_string(&restore_prefix));
        self.pending_prompt_cache_tool_hash = Some(hash_string(&tool_prefix));
        self.pending_prompt_cache_message_hash = Some(hash_string(&message_prefix));
        self.pending_prompt_cache_system_text = Some(system_prefix.clone());
        self.pending_prompt_cache_restore_text = Some(restore_prefix.clone());
        self.pending_prompt_cache_tool_text = Some(tool_prefix.clone());
        self.pending_prompt_cache_message_text = Some(message_prefix.clone());
        self.pending_prompt_cache_prefix_hash = Some(hash_string(&format!(
            "{}\n{}\n{}\n{}",
            system_prefix, restore_prefix, tool_prefix, message_prefix
        )));
        self.pending_prompt_cache_expected_drop_reason = self
            .forced_prompt_cache_expected_drop_reason
            .clone()
            .or_else(|| {
                request.provider_hints.anthropic.as_ref().and_then(|hints| {
                    (!hints.pending_deleted_cache_references.is_empty())
                        .then(|| "cache_edits".to_string())
                })
            });
        if let Some(hints) = request.provider_hints.anthropic.as_ref() {
            self.prompt_cache_runtime.pending_cache_edit_refs =
                hints.pending_deleted_cache_references.len() as u32;
            self.prompt_cache_runtime.pinned_cache_edit_refs =
                hints.pinned_deleted_cache_references.len() as u32;
        } else {
            self.prompt_cache_runtime.pending_cache_edit_refs = 0;
            self.prompt_cache_runtime.pinned_cache_edit_refs = 0;
        }
        self.prompt_cache_runtime
            .last_prompt_cache_expected_drop_reason =
            self.pending_prompt_cache_expected_drop_reason.clone();
    }

    async fn detect_prompt_cache_break(
        &mut self,
        previous_cache_read_tokens: Option<u32>,
        current_cache_read_tokens: u32,
    ) {
        let previous_hash = self.last_prompt_cache_prefix_hash.clone();
        let current_hash = self.pending_prompt_cache_prefix_hash.clone();
        let previous_read = previous_cache_read_tokens.unwrap_or(0);
        let expected_reason = self.pending_prompt_cache_expected_drop_reason.clone();
        let change_summary = prompt_cache_change_summary(
            self.last_prompt_cache_system_hash.as_deref(),
            self.last_prompt_cache_restore_hash.as_deref(),
            self.last_prompt_cache_tool_hash.as_deref(),
            self.last_prompt_cache_message_hash.as_deref(),
            self.pending_prompt_cache_system_hash.as_deref(),
            self.pending_prompt_cache_restore_hash.as_deref(),
            self.pending_prompt_cache_tool_hash.as_deref(),
            self.pending_prompt_cache_message_hash.as_deref(),
        );
        self.prompt_cache_runtime.last_prompt_cache_change_summary = Some(change_summary);
        if previous_hash.is_none() {
            self.prompt_cache_runtime.last_prompt_cache_transition_kind =
                Some("cold_start".to_string());
            self.prompt_cache_runtime
                .last_prompt_cache_transition_reason =
                Some("no previous prompt cache snapshot".to_string());
        } else {
            self.prompt_cache_runtime.last_prompt_cache_transition_kind =
                Some("stable".to_string());
            self.prompt_cache_runtime
                .last_prompt_cache_transition_reason = None;
        }

        if let Some(reason) = expected_reason.clone() {
            self.prompt_cache_runtime.last_prompt_cache_transition_kind = Some(
                if reason == "cache_edits" {
                    "cache_edit_applied"
                } else {
                    "expected_drop"
                }
                .to_string(),
            );
            self.prompt_cache_runtime
                .last_prompt_cache_transition_reason = Some(reason);
        } else if previous_hash.is_some() && current_hash.is_some() && previous_hash != current_hash
        {
            let transition_kind = prompt_cache_transition_kind_for_change_summary(
                self.prompt_cache_runtime
                    .last_prompt_cache_change_summary
                    .as_deref()
                    .unwrap_or("prefix_changed"),
            );
            self.prompt_cache_runtime.last_prompt_cache_transition_kind =
                Some(transition_kind.to_string());
            self.prompt_cache_runtime
                .last_prompt_cache_transition_reason = Some(
                self.prompt_cache_runtime
                    .last_prompt_cache_change_summary
                    .clone()
                    .unwrap_or_else(|| "prefix changed".to_string()),
            );
        }

        if previous_read > 0
            && expected_reason.is_none()
            && previous_hash.is_some()
            && current_hash.is_some()
            && previous_hash == current_hash
        {
            let dropped = previous_read.saturating_sub(current_cache_read_tokens);
            let threshold = (previous_read / 2).max(1_000);
            if dropped >= threshold {
                self.prompt_cache_runtime.prompt_cache_break_count = self
                    .prompt_cache_runtime
                    .prompt_cache_break_count
                    .saturating_add(1);
                self.prompt_cache_runtime.last_prompt_cache_break_reason = Some(format!(
                    "cache read dropped from {} to {} while prefix hash stayed stable",
                    previous_read, current_cache_read_tokens
                ));
                self.prompt_cache_runtime.last_prompt_cache_break_at = Some(Self::now_timestamp());
                self.prompt_cache_runtime.last_prompt_cache_transition_kind =
                    Some("break".to_string());
                self.prompt_cache_runtime
                    .last_prompt_cache_transition_reason = self
                    .prompt_cache_runtime
                    .last_prompt_cache_break_reason
                    .clone();
            }
        }

        let transition_kind = self
            .prompt_cache_runtime
            .last_prompt_cache_transition_kind
            .clone()
            .unwrap_or_else(|| "stable".to_string());
        let transition_reason = self
            .prompt_cache_runtime
            .last_prompt_cache_transition_reason
            .clone();
        if transition_kind != "stable" {
            match write_prompt_cache_diff_artifact_async(
                &self.context.working_dir_compat(),
                &self.context.session_id,
                &transition_kind,
                transition_reason.as_deref(),
                self.last_prompt_cache_system_hash.as_deref(),
                self.last_prompt_cache_restore_hash.as_deref(),
                self.last_prompt_cache_tool_hash.as_deref(),
                self.last_prompt_cache_message_hash.as_deref(),
                self.pending_prompt_cache_system_hash.as_deref(),
                self.pending_prompt_cache_restore_hash.as_deref(),
                self.pending_prompt_cache_tool_hash.as_deref(),
                self.pending_prompt_cache_message_hash.as_deref(),
                self.last_prompt_cache_system_text.as_deref(),
                self.last_prompt_cache_restore_text.as_deref(),
                self.last_prompt_cache_tool_text.as_deref(),
                self.last_prompt_cache_message_text.as_deref(),
                self.pending_prompt_cache_system_text.as_deref(),
                self.pending_prompt_cache_restore_text.as_deref(),
                self.pending_prompt_cache_tool_text.as_deref(),
                self.pending_prompt_cache_message_text.as_deref(),
            )
            .await
            {
                Ok((path, summary)) => {
                    self.prompt_cache_runtime
                        .last_prompt_cache_diff_artifact_path = Some(path);
                    self.prompt_cache_runtime.last_prompt_cache_diff_summary = Some(summary);
                }
                Err(err) => {
                    tracing::warn!("Failed to write prompt-cache diff artifact: {}", err);
                }
            }
        }

        self.last_prompt_cache_prefix_hash = current_hash;
        self.last_prompt_cache_system_hash = self.pending_prompt_cache_system_hash.take();
        self.last_prompt_cache_restore_hash = self.pending_prompt_cache_restore_hash.take();
        self.last_prompt_cache_tool_hash = self.pending_prompt_cache_tool_hash.take();
        self.last_prompt_cache_message_hash = self.pending_prompt_cache_message_hash.take();
        self.last_prompt_cache_system_text = self.pending_prompt_cache_system_text.take();
        self.last_prompt_cache_restore_text = self.pending_prompt_cache_restore_text.take();
        self.last_prompt_cache_tool_text = self.pending_prompt_cache_tool_text.take();
        self.last_prompt_cache_message_text = self.pending_prompt_cache_message_text.take();
        self.pending_prompt_cache_prefix_hash = None;
        self.pending_prompt_cache_expected_drop_reason = None;
        self.clear_expected_prompt_cache_drop_reason();
    }

    pub(super) fn clear_cache_edit_tracking(&mut self) {
        self.cached_microcompact_deleted_refs.clear();
        self.pending_cache_edit_refs.clear();
        self.pinned_cache_edit_refs.clear();
        self.pending_prompt_cache_prefix_hash = None;
        self.last_prompt_cache_prefix_hash = None;
        self.pending_prompt_cache_system_hash = None;
        self.pending_prompt_cache_restore_hash = None;
        self.pending_prompt_cache_tool_hash = None;
        self.pending_prompt_cache_message_hash = None;
        self.last_prompt_cache_system_hash = None;
        self.last_prompt_cache_restore_hash = None;
        self.last_prompt_cache_tool_hash = None;
        self.last_prompt_cache_message_hash = None;
        self.pending_prompt_cache_system_text = None;
        self.pending_prompt_cache_restore_text = None;
        self.pending_prompt_cache_tool_text = None;
        self.pending_prompt_cache_message_text = None;
        self.last_prompt_cache_system_text = None;
        self.last_prompt_cache_restore_text = None;
        self.last_prompt_cache_tool_text = None;
        self.last_prompt_cache_message_text = None;
        self.pending_prompt_cache_expected_drop_reason = None;
        self.clear_expected_prompt_cache_drop_reason();
        self.prompt_cache_runtime.pending_cache_edit_refs = 0;
        self.prompt_cache_runtime.pinned_cache_edit_refs = 0;
        self.prompt_cache_runtime.last_prompt_cache_change_summary = None;
        self.prompt_cache_runtime.last_prompt_cache_transition_kind = None;
        self.prompt_cache_runtime
            .last_prompt_cache_transition_reason = None;
        self.prompt_cache_runtime
            .last_prompt_cache_diff_artifact_path = None;
        self.prompt_cache_runtime.last_prompt_cache_diff_summary = None;
    }

    pub(super) fn record_tool_progress_summary(
        &mut self,
        tool_name: &str,
        count: u32,
        last_message: Option<String>,
    ) {
        if count == 0 {
            return;
        }
        self.tool_progress_event_count = self.tool_progress_event_count.saturating_add(count);
        self.current_turn_tool_progress_events =
            self.current_turn_tool_progress_events.saturating_add(count);
        if let Some(message) = last_message {
            self.last_tool_progress_message = Some(message);
            self.last_tool_progress_tool = Some(tool_name.to_string());
            self.last_tool_progress_at = Some(Self::now_timestamp());
        }
    }

    pub(super) fn register_parallel_batch(&mut self, batch_size: usize) -> u32 {
        self.parallel_tool_batch_count = self.parallel_tool_batch_count.saturating_add(1);
        self.current_turn_parallel_batches = self.current_turn_parallel_batches.saturating_add(1);
        self.parallel_tool_call_count = self
            .parallel_tool_call_count
            .saturating_add(batch_size as u32);
        self.current_turn_parallel_calls = self
            .current_turn_parallel_calls
            .saturating_add(batch_size as u32);
        self.max_parallel_batch_size = self.max_parallel_batch_size.max(batch_size);
        self.current_turn_max_parallel_batch_size =
            self.current_turn_max_parallel_batch_size.max(batch_size);
        self.parallel_tool_batch_count
    }

    pub(super) fn maybe_record_tool_budget_warning(&mut self) -> Option<String> {
        if self.tool_call_count >= TOOL_BUDGET_WARNING && !self.current_turn_budget_warning_emitted
        {
            let message =
                "工具调用提醒：本轮已使用 25 次工具调用。请停止继续探索，直接用中文总结已完成、未完成、修改文件和建议下一步；不要只说预算耗尽。";
            self.current_turn_budget_warning_emitted = true;
            self.tool_budget_warning_count = self.tool_budget_warning_count.saturating_add(1);
            self.last_tool_budget_warning = Some(message.to_string());
            return Some(message.to_string());
        }

        if self.tool_call_count >= TOOL_BUDGET_NOTICE && !self.current_turn_budget_notice_emitted {
            let message =
                "工具调用提醒：本轮已使用 15 次工具调用。继续前请优先收敛任务，必要时用中文简短总结当前发现和下一步。";
            self.current_turn_budget_notice_emitted = true;
            self.tool_budget_notice_count = self.tool_budget_notice_count.saturating_add(1);
            self.last_tool_budget_warning = Some(message.to_string());
            return Some(message.to_string());
        }

        None
    }

    fn note_tool_truncation(&mut self, truncation: &crate::tool_runtime::ToolResultTruncationView) {
        self.tool_truncation_count = self.tool_truncation_count.saturating_add(1);
        self.current_turn_truncated_results = self.current_turn_truncated_results.saturating_add(1);
        self.last_tool_truncation_reason = Some(truncation.reason.clone());
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "tool execution trace records execution timing, batching, progress, and input size together"
    )]
    pub(super) fn record_tool_execution_trace(
        &mut self,
        tool_call: &ToolCall,
        result: &ToolResult,
        started_at: Option<String>,
        duration_ms: u64,
        progress_updates: u32,
        parallel_batch: Option<u32>,
        input_bytes: usize,
    ) {
        let error_type = result.error_type.map(|kind| format!("{:?}", kind));
        if let Some(kind) = error_type.clone() {
            *self.tool_error_type_counts.entry(kind.clone()).or_insert(0) += 1;
        }

        let repeated_failure_count = if result.is_error {
            let signature = failure_signature(tool_call, error_type.as_deref());
            let count = self
                .repeated_tool_failure_patterns
                .entry(signature)
                .and_modify(|existing| *existing = existing.saturating_add(1))
                .or_insert(1);
            if *count >= 2 {
                self.latest_repeated_tool_failure = Some(format!(
                    "{} [{}] x{}",
                    tool_call.name,
                    error_type.as_deref().unwrap_or("unknown"),
                    *count
                ));
            }
            *count
        } else {
            0
        };

        let truncation = tool_truncation_from_metadata(&result.metadata);
        if let Some(ref truncation) = truncation {
            self.note_tool_truncation(truncation);
        }
        let trace = ToolExecutionTrace {
            call_id: tool_call.id.clone(),
            tool_name: tool_call.name.clone(),
            started_at,
            duration_ms,
            input_bytes,
            output_bytes: result.content.len(),
            progress_updates,
            success: !result.is_error,
            error_type,
            parallel_batch,
            truncation,
            repeated_failure_count,
            metadata_summary: summarize_result_metadata(&result.metadata),
            diff_preview: extract_diff_preview(&result.metadata),
            output_preview: output_preview(&result.content),
        };
        self.current_tool_execution_traces.push(trace);
    }

    #[cfg(test)]
    pub(super) fn complete_tool_turn_artifact(&mut self) {
        if self.current_tool_execution_traces.is_empty() {
            self.current_tool_turn_started_at = None;
            return;
        }

        let total_calls = self.current_tool_execution_traces.len() as u32;
        let success_count = self
            .current_tool_execution_traces
            .iter()
            .filter(|trace| trace.success)
            .count() as u32;
        let failed_count = total_calls.saturating_sub(success_count);
        let mut current_error_type_counts = BTreeMap::new();
        for trace in &self.current_tool_execution_traces {
            if let Some(kind) = trace.error_type.as_ref() {
                *current_error_type_counts.entry(kind.clone()).or_insert(0) += 1;
            }
        }
        let inventory = self.tools.inventory();
        let tool_pool_snapshot = self.build_tool_pool_snapshot();

        let artifact = ToolTurnArtifact {
            turn_index: self.tool_turn_counter,
            started_at: self.current_tool_turn_started_at.clone(),
            completed_at: Some(Self::now_timestamp()),
            total_calls,
            success_count,
            failed_count,
            total_output_bytes: self.total_tool_results_bytes,
            truncated_results: self.current_turn_truncated_results,
            progress_events: self.current_turn_tool_progress_events,
            parallel_batches: self.current_turn_parallel_batches,
            parallel_calls: self.current_turn_parallel_calls,
            max_parallel_batch_size: self.current_turn_max_parallel_batch_size,
            budget_notice_emitted: self.current_turn_budget_notice_emitted,
            budget_warning_emitted: self.current_turn_budget_warning_emitted,
            last_budget_warning: self.last_tool_budget_warning.clone(),
            latest_repeated_failure: self.latest_repeated_tool_failure.clone(),
            error_type_counts: current_error_type_counts,
            tool_pool: Some(ToolPoolArtifactView::from_snapshot(
                &tool_pool_snapshot,
                inventory.activation_count,
                inventory.last_activated_tool,
            )),
            calls: self
                .current_tool_execution_traces
                .iter()
                .map(ToolExecutionTrace::to_view)
                .collect(),
        };

        if let Ok(path) = write_tool_turn_artifact(
            &self.context.working_dir_compat(),
            &self.context.session_id,
            &artifact,
        ) {
            self.last_tool_turn_artifact_path = Some(path.display().to_string());
        }

        self.last_tool_turn_completed_at = artifact.completed_at.clone();
        self.last_tool_turn_traces = self.current_tool_execution_traces.clone();
        self.current_tool_execution_traces.clear();
        self.current_tool_turn_started_at = None;
    }

    pub(super) async fn complete_tool_turn_artifact_async(&mut self) {
        if self.current_tool_execution_traces.is_empty() {
            self.current_tool_turn_started_at = None;
            return;
        }

        let total_calls = self.current_tool_execution_traces.len() as u32;
        let success_count = self
            .current_tool_execution_traces
            .iter()
            .filter(|trace| trace.success)
            .count() as u32;
        let failed_count = total_calls.saturating_sub(success_count);
        let mut current_error_type_counts = BTreeMap::new();
        for trace in &self.current_tool_execution_traces {
            if let Some(kind) = trace.error_type.as_ref() {
                *current_error_type_counts.entry(kind.clone()).or_insert(0) += 1;
            }
        }
        let inventory = self.tools.inventory();
        let tool_pool_snapshot = self.build_tool_pool_snapshot();

        let artifact = ToolTurnArtifact {
            turn_index: self.tool_turn_counter,
            started_at: self.current_tool_turn_started_at.clone(),
            completed_at: Some(Self::now_timestamp()),
            total_calls,
            success_count,
            failed_count,
            total_output_bytes: self.total_tool_results_bytes,
            truncated_results: self.current_turn_truncated_results,
            progress_events: self.current_turn_tool_progress_events,
            parallel_batches: self.current_turn_parallel_batches,
            parallel_calls: self.current_turn_parallel_calls,
            max_parallel_batch_size: self.current_turn_max_parallel_batch_size,
            budget_notice_emitted: self.current_turn_budget_notice_emitted,
            budget_warning_emitted: self.current_turn_budget_warning_emitted,
            last_budget_warning: self.last_tool_budget_warning.clone(),
            latest_repeated_failure: self.latest_repeated_tool_failure.clone(),
            error_type_counts: current_error_type_counts,
            tool_pool: Some(ToolPoolArtifactView::from_snapshot(
                &tool_pool_snapshot,
                inventory.activation_count,
                inventory.last_activated_tool,
            )),
            calls: self
                .current_tool_execution_traces
                .iter()
                .map(ToolExecutionTrace::to_view)
                .collect(),
        };

        if let Ok(path) = write_tool_turn_artifact_async(
            &self.context.working_dir_compat(),
            &self.context.session_id,
            &artifact,
        )
        .await
        {
            self.last_tool_turn_artifact_path = Some(path.display().to_string());
        }

        self.last_tool_turn_completed_at = artifact.completed_at.clone();
        self.last_tool_turn_traces = self.current_tool_execution_traces.clone();
        self.current_tool_execution_traces.clear();
        self.current_tool_turn_started_at = None;
    }
}
