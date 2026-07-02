use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::context_manager::CompressionReport;
use yode_llm::types::{ChatRequest, Message, RestoreSystemBlockHint, Role};

use super::*;

pub(super) mod blocks;
pub(super) mod budget;
pub(super) mod summarizer;

pub(super) use crate::engine::types::RestoreBudgetRuntimeState;
use budget::apply_restore_budget;

use blocks::{
    collect_preserved_read_file_paths, load_post_compact_restore_state_artifact,
    load_post_compact_restore_state_artifact_async, ordered_restore_block_contents,
    render_post_compact_file_excerpts, render_skill_invocation_restore_lines,
    render_task_restore_lines, restore_block_body, restore_block_kind_from_content,
    sanitized_request_restore_block_contents, write_post_compact_restore_artifact_async,
    write_post_compact_restore_diff_artifact_async,
    write_post_compact_restore_state_artifact_async, RestoreBlockKind,
    POST_COMPACT_ARTIFACTS_PREFIX, POST_COMPACT_FILES_PREFIX, POST_COMPACT_MCP_PREFIX,
    POST_COMPACT_PLAN_PREFIX, POST_COMPACT_PROMPT_CACHE_PREFIX, POST_COMPACT_RUNTIME_PREFIX,
    POST_COMPACT_SKILLS_PREFIX, POST_COMPACT_TOOLS_PREFIX,
};
use summarizer::{
    build_fallback_compaction_summary, build_session_memory_compaction_summary,
    collect_assistant_tool_call_ids, collect_tool_result_ids, compact_summary_fingerprint,
    display_compaction_memory_path, format_llm_compaction_summary_content,
    is_media_size_error_text, is_prompt_too_long_text, parse_prompt_too_long_token_gap,
    preserved_tail_range, prompt_cache_text_value, prompt_cache_value, push_artifact_path,
    render_removed_messages_for_summary, summarize_string_entries, truncate_head_for_summary_retry,
    CompactionMode, CompactionSummaryScope, LLM_COMPACTION_MAX_RETRIES,
    LLM_COMPACTION_SUMMARY_MAX_CHARS, LLM_COMPACTION_TRANSCRIPT_CHAR_BUDGET,
    SESSION_MEMORY_SUMMARY_PREFIX,
};

const REACTIVE_GAP_SAFETY_TOKENS: usize = 2_000;

impl AgentEngine {
    fn take_post_compact_restore_messages_from_conversation(
        &mut self,
    ) -> Vec<(RestoreBlockKind, String)> {
        let mut extracted = Vec::new();
        self.messages.retain(|message| {
            let Some(content) = message.content.as_deref() else {
                return true;
            };
            let Some(kind) = restore_block_kind_from_content(content) else {
                return true;
            };
            extracted.push((kind, content.to_string()));
            false
        });
        extracted
    }

    fn set_post_compact_restore_blocks(&mut self, contents: Vec<(RestoreBlockKind, String)>) {
        self.post_compact_restore_blocks = ordered_restore_block_contents(contents);
    }

    pub(super) fn request_restore_system_blocks(&self) -> Vec<RestoreSystemBlockHint> {
        sanitized_request_restore_block_contents(self.post_compact_restore_blocks.clone())
            .into_iter()
            .filter_map(|content| {
                let kind = restore_block_kind_from_content(&content)?;
                Some(RestoreSystemBlockHint {
                    kind: kind.label().to_string(),
                    content: restore_block_body(&content).to_string(),
                })
            })
            .collect()
    }

    pub(super) fn hidden_post_compact_restore_prompt_text(&self) -> Option<String> {
        blocks::render_hidden_post_compact_restore_prompt(self.request_restore_system_blocks())
    }

    pub(super) fn rehydrate_post_compact_restore_messages(&mut self) {
        let extracted = self.take_post_compact_restore_messages_from_conversation();
        if !extracted.is_empty() {
            self.set_post_compact_restore_blocks(extracted);
        }

        let has_summary_anchor = self.messages.iter().any(|message| {
            matches!(message.role, Role::System)
                && message
                    .content
                    .as_deref()
                    .unwrap_or_default()
                    .starts_with(SESSION_MEMORY_SUMMARY_PREFIX)
        });
        if !has_summary_anchor {
            self.post_compact_restore_blocks.clear();
            return;
        }

        if !self.post_compact_restore_blocks.is_empty() {
            return;
        }

        if let Some(blocks) = load_post_compact_restore_state_artifact(
            &self.context.working_dir_compat(),
            &self.context.session_id,
        ) {
            self.set_post_compact_restore_blocks(blocks);
        }
    }

    pub(super) fn apply_microcompact(&mut self) {
        let media_report = self
            .context_manager
            .microcompact_old_media(&mut self.messages);
        let media_changed = media_report.media_removed > 0;
        self.last_microcompact_media_removed = media_report.media_removed as u32;
        self.last_microcompact_media_saved_chars = media_report.saved_chars as u64;
        if media_changed {
            self.microcompact_media_removed_total = self
                .microcompact_media_removed_total
                .saturating_add(media_report.media_removed as u64);
            self.microcompact_media_saved_chars_total = self
                .microcompact_media_saved_chars_total
                .saturating_add(media_report.saved_chars as u64);
        }

        if self.supports_anthropic_cache_editing() {
            let refs = self
                .context_manager
                .collect_microcompact_cache_refs(&self.messages);
            if refs.is_empty() {
                self.cached_microcompact_deleted_refs.clear();
                self.pending_cache_edit_refs.clear();
                self.prompt_cache_runtime.last_turn_cache_edit_deletions = Some(0);
                if media_changed {
                    self.record_compaction_cause("microcompact_media");
                    self.sync_persisted_messages_snapshot();
                    debug!(
                        "Applied media microcompact: removed {} old attachment(s) and saved ~{} chars",
                        media_report.media_removed, media_report.saved_chars
                    );
                }
                return;
            }
            self.cached_microcompact_deleted_refs = refs.clone();
            self.pending_cache_edit_refs = refs
                .into_iter()
                .filter(|cache_ref| !self.pinned_cache_edit_refs.contains(cache_ref))
                .collect();
            let deletions =
                self.pending_cache_edit_refs
                    .len()
                    .saturating_add(self.pinned_cache_edit_refs.len()) as u32;
            self.prompt_cache_runtime.last_turn_cache_edit_deletions = Some(deletions);
            self.prompt_cache_runtime.cache_edit_turns =
                self.prompt_cache_runtime.cache_edit_turns.saturating_add(1);
            self.prompt_cache_runtime.cache_edit_deletions_total = self
                .prompt_cache_runtime
                .cache_edit_deletions_total
                .saturating_add(deletions as u64);
            self.prompt_cache_runtime.pending_cache_edit_refs =
                self.pending_cache_edit_refs.len() as u32;
            self.prompt_cache_runtime.pinned_cache_edit_refs =
                self.pinned_cache_edit_refs.len() as u32;
            self.record_compaction_cause("microcompact_cached");
            if media_changed {
                self.record_compaction_cause("microcompact_media");
                self.sync_persisted_messages_snapshot();
                debug!(
                    "Applied media microcompact: removed {} old attachment(s) and saved ~{} chars",
                    media_report.media_removed, media_report.saved_chars
                );
            }
            debug!(
                "Prepared cached microcompact with {} total cache references ({} pending, {} pinned)",
                self.cached_microcompact_deleted_refs.len(),
                self.pending_cache_edit_refs.len(),
                self.pinned_cache_edit_refs.len()
            );
            return;
        }

        let report = self.context_manager.microcompact(&mut self.messages);
        if report.tool_results_cleared == 0 && !media_changed {
            return;
        }

        self.cached_microcompact_deleted_refs.clear();
        self.pending_cache_edit_refs.clear();
        self.pinned_cache_edit_refs.clear();
        self.prompt_cache_runtime.last_turn_cache_edit_deletions = Some(0);
        self.prompt_cache_runtime.pending_cache_edit_refs = 0;
        self.prompt_cache_runtime.pinned_cache_edit_refs = 0;
        if report.tool_results_cleared > 0 {
            self.record_compaction_cause("microcompact");
        }
        if media_changed {
            self.record_compaction_cause("microcompact_media");
        }
        self.sync_persisted_messages_snapshot();
        debug!(
            "Applied microcompact: cleared {} older tool results, removed {} old attachment(s), and saved ~{} chars",
            report.tool_results_cleared,
            media_report.media_removed,
            report.saved_chars.saturating_add(media_report.saved_chars)
        );
    }

    pub(super) async fn maybe_compact_context(
        &mut self,
        prompt_tokens: u32,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) {
        let prompt_tokens = if self.apply_context_collapse_if_enabled().await {
            self.estimated_prompt_tokens_for_current_messages()
        } else {
            prompt_tokens
        };
        let _ = self
            .compact_context(prompt_tokens, event_tx, CompactionMode::Auto, None)
            .await;
    }

    async fn apply_context_collapse_if_enabled(&mut self) -> bool {
        if !crate::context_collapse::is_context_collapse_enabled() {
            return false;
        }
        let Some(operation) =
            crate::context_collapse::collapse_tool_heavy_spans(&mut self.messages)
        else {
            return false;
        };

        let project_root = self.context.working_dir_compat();
        match crate::context_collapse::write_context_collapse_artifact_async(
            &project_root,
            &self.context.session_id,
            &operation,
        )
        .await
        {
            Ok(path) => {
                self.last_context_collapse_artifact_path = Some(path.display().to_string());
            }
            Err(err) => warn!("Failed to write context collapse artifact: {}", err),
        }
        self.last_context_collapse_at = Some(operation.created_at);
        self.last_context_collapse_saved_chars = operation.saved_chars as u64;
        self.context_collapse_saved_chars_total = self
            .context_collapse_saved_chars_total
            .saturating_add(operation.saved_chars as u64);
        self.context_collapse_operations = self.context_collapse_operations.saturating_add(1);
        self.record_compaction_cause("context_collapse");
        self.sync_persisted_messages_snapshot();
        true
    }

    pub(super) async fn reactive_compact_context_for_text(
        &mut self,
        error_text: &str,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) -> bool {
        if self.reactive_compact_attempted {
            return false;
        }
        self.reactive_compact_attempted = true;

        if let Some(end) = self.reactive_prefix_end_for_token_gap(error_text) {
            if self.partial_compact_range(1, end, event_tx).await {
                self.record_compaction_cause("reactive_prefix_compact");
                return true;
            }
        }

        let estimated_tokens = self.estimated_prompt_tokens_for_current_messages();
        self.compact_context(estimated_tokens, event_tx, CompactionMode::Reactive, None)
            .await
    }

    pub(super) fn should_reactive_compact_error(&self, err: &anyhow::Error) -> bool {
        !self.reactive_compact_attempted && is_prompt_too_long_text(&format!("{:#}", err))
    }

    pub(super) fn should_reactive_compact_message(&self, message: &str) -> bool {
        !self.reactive_compact_attempted && is_prompt_too_long_text(message)
    }

    pub(super) fn should_reactive_strip_media_error(&self, err: &anyhow::Error) -> bool {
        !self.reactive_media_strip_attempted && is_media_size_error_text(&format!("{:#}", err))
    }

    pub(super) fn should_reactive_strip_media_message(&self, message: &str) -> bool {
        !self.reactive_media_strip_attempted && is_media_size_error_text(message)
    }

    pub(super) fn reactive_prefix_end_for_token_gap(&self, error_text: &str) -> Option<usize> {
        let gap = parse_prompt_too_long_token_gap(error_text)?;
        let target = gap.saturating_add(REACTIVE_GAP_SAFETY_TOKENS);
        if self.messages.len() <= 8 {
            return None;
        }

        let keep_tail = 8usize;
        let max_end = self.messages.len().saturating_sub(keep_tail);
        if max_end <= 2 {
            return None;
        }

        let mut accumulated = 0usize;
        for idx in 1..max_end {
            accumulated = accumulated.saturating_add(self.messages[idx].estimated_char_count() / 4);
            if accumulated >= target {
                return Some((idx + 1).min(max_end));
            }
        }

        None
    }

    pub(super) fn reactive_strip_old_media(&mut self) -> bool {
        let preserve_recent = 6usize;
        if self.messages.len() <= preserve_recent + 1 {
            return false;
        }

        let mut changed = false;
        let cutoff = self.messages.len().saturating_sub(preserve_recent);
        for message in self.messages.iter_mut().take(cutoff).skip(1) {
            if message.images.is_empty() {
                continue;
            }

            message.images.clear();
            let marker = "[older media removed after API size rejection]";
            match message.content.as_mut() {
                Some(content) if !content.contains(marker) => {
                    content.push_str("\n\n");
                    content.push_str(marker);
                }
                None => {
                    message.content = Some(marker.to_string());
                }
                _ => {}
            }
            message.normalize_in_place();
            changed = true;
        }

        if changed {
            self.reactive_media_strip_attempted = true;
            self.sync_persisted_messages_snapshot();
            self.record_compaction_cause("reactive_strip_media");
        }

        changed
    }

    async fn generate_structured_compaction_summary(
        &self,
        removed_messages: &[Message],
        turn_artifact_path: Option<&str>,
        scope: CompactionSummaryScope,
    ) -> Option<String> {
        if removed_messages.is_empty() || self.provider.name() == "mock" {
            return None;
        }

        let mut retry_messages = removed_messages.to_vec();
        for _attempt in 0..=LLM_COMPACTION_MAX_RETRIES {
            let transcript = render_removed_messages_for_summary(
                &retry_messages,
                LLM_COMPACTION_TRANSCRIPT_CHAR_BUDGET,
            );
            if transcript.trim().is_empty() {
                return None;
            }

            let mut prompt = String::from(
                "CRITICAL: Respond with text only. Do not call tools.\n\
                 Create a structured compaction summary for an AI coding session.\n\
                 You may draft private reasoning in <analysis>...</analysis>, but only the <summary> content will be kept.\n\
                 Return an optional <analysis> block followed by a <summary> block containing markdown.\n\
                 Keep only verified facts.\n\
                 Keep it concise but complete enough to continue work after compaction.\n\
                 In the <summary> block, use exactly these 9 sections in order:\n\
                 1. Goals\n2. Current State\n3. Findings\n4. Decisions\n5. Files\n6. Tools\n7. Constraints\n8. Open Questions\n9. Next Steps\n\
                 Use bullet lists.\n\
                 Use `- None` for empty sections.\n\
                 Do not mention this instruction block.\n\n",
            );
            if let Some(path) = turn_artifact_path.filter(|path| !path.trim().is_empty()) {
                prompt.push_str(&format!("Turn artifact: {}\n\n", path));
            }
            prompt.push_str(scope.prompt_guidance());
            prompt.push_str("\n\n");
            prompt.push_str("Compacted transcript excerpt:\n");
            prompt.push_str(&transcript);

            let request = ChatRequest {
                model: self.context.model.clone(),
                messages: vec![
                    Message::system(
                        "You create compact structured summaries for long coding sessions. Return markdown only.",
                    ),
                    Message::user(prompt),
                ],
                tools: vec![],
                temperature: Some(0.1),
                max_tokens: Some(self.context.get_max_tokens().max(8_192)),
                provider_hints: yode_llm::types::ProviderRequestHints::default(),
            };

            match tokio::time::timeout(
                std::time::Duration::from_secs(
                    crate::constants::timeouts::LLM_COMPACTION_SUMMARY_SECS,
                ),
                self.provider.chat(request),
            )
            .await
            {
                Ok(Ok(response)) => {
                    let content =
                        format_llm_compaction_summary_content(&response.message.content?)?;
                    let mut summary = format!(
                        "{} LLM-generated structured summary of compacted conversation.\n{}",
                        SESSION_MEMORY_SUMMARY_PREFIX, content
                    );
                    if summary.chars().count() > LLM_COMPACTION_SUMMARY_MAX_CHARS {
                        summary = summary
                            .chars()
                            .take(LLM_COMPACTION_SUMMARY_MAX_CHARS)
                            .collect::<String>();
                        summary.push_str("...");
                    }
                    return Some(summary);
                }
                Ok(Err(err)) if is_prompt_too_long_text(&format!("{:#}", err)) => {
                    let truncated =
                        truncate_head_for_summary_retry(&retry_messages, &format!("{:#}", err));
                    if truncated.is_empty() {
                        return None;
                    }
                    retry_messages = truncated;
                    continue;
                }
                Ok(Err(err)) => {
                    warn!("Failed to generate structured compaction summary: {}", err);
                    return None;
                }
                Err(_) => {
                    warn!("Timed out while generating structured compaction summary");
                    return None;
                }
            }
        }

        None
    }

    fn replace_compaction_summary_message(
        &mut self,
        previous_summary: Option<&str>,
        new_summary: &str,
    ) {
        if let Some(previous_summary) = previous_summary {
            if let Some(message) = self.messages.iter_mut().find(|message| {
                matches!(message.role, Role::System)
                    && message.content.as_deref() == Some(previous_summary)
            }) {
                *message = Message::system(new_summary.to_string());
                return;
            }
        }

        if let Some(message) = self.messages.iter_mut().rev().find(|message| {
            matches!(message.role, Role::System)
                && message
                    .content
                    .as_deref()
                    .unwrap_or_default()
                    .starts_with(SESSION_MEMORY_SUMMARY_PREFIX)
        }) {
            *message = Message::system(new_summary.to_string());
        }
    }

    async fn build_post_compact_restore_messages(
        &self,
        mode: CompactionMode,
        session_memory_path: Option<&std::path::Path>,
        transcript_path: Option<&std::path::Path>,
        post_compact_estimated_tokens: Option<u32>,
        auto_compact_threshold: Option<u32>,
        will_retrigger_next_turn: Option<bool>,
    ) -> (Vec<(RestoreBlockKind, String)>, RestoreBudgetRuntimeState) {
        let project_root = self.context.working_dir_compat();
        let cwd = self.current_runtime_working_dir().await;
        let tool_pool = self.build_tool_pool_snapshot();
        let inventory = self.tools.inventory();
        let plan_snapshot = self.plan_runtime_state();
        let skill_paths = crate::skills::SkillRegistry::default_paths_async(&project_root).await;
        let skills = crate::skills::SkillRegistry::discover_async(&skill_paths).await;
        let mcp_cache = yode_tools::mcp_resource_cache_stats();

        let read_files = ordered_recent_read_files(&self.recent_file_reads, &self.files_read);
        let modified_files = self.files_modified.clone();
        let recent_paths = read_files
            .iter()
            .chain(modified_files.iter())
            .cloned()
            .collect::<Vec<_>>();
        let active_skills = skills.active_for_paths(recent_paths.iter());
        let skill_names = skills
            .list()
            .iter()
            .take(5)
            .map(|skill| skill.name.clone())
            .collect::<Vec<_>>();
        let skill_invocations = self.skill_invocation_store.lock().await.clone();

        let mut runtime_lines = vec![
            format!(
                "{} Re-injected runtime context after {} compaction.",
                POST_COMPACT_RUNTIME_PREFIX,
                mode.label()
            ),
            format!("- Runtime cwd: {}", cwd),
            "- Persistent memory and instruction context remain available via the system prompt."
                .to_string(),
        ];
        if let (Some(estimated_tokens), Some(threshold), Some(will_retrigger)) = (
            post_compact_estimated_tokens,
            auto_compact_threshold,
            will_retrigger_next_turn,
        ) {
            let delta = estimated_tokens as i64 - threshold as i64;
            runtime_lines.push(format!(
                "- Post-compact pressure: est={} threshold={} delta={} next_auto={}",
                estimated_tokens,
                threshold,
                delta,
                if will_retrigger { "likely" } else { "clear" }
            ));
        }

        let mut file_lines = vec![POST_COMPACT_FILES_PREFIX.to_string()];
        if let Some(summary) = summarize_string_entries(&read_files, 5) {
            file_lines.push(format!("- Recent files read: {}", summary));
        }
        if let Some(summary) = summarize_string_entries(&modified_files, 5) {
            file_lines.push(format!("- Recent files modified: {}", summary));
        }
        let preserved_read_files = collect_preserved_read_file_paths(&self.messages);
        let file_excerpts = render_post_compact_file_excerpts(
            &project_root,
            &cwd,
            &read_files,
            &preserved_read_files,
        )
        .await;
        if !file_excerpts.is_empty() {
            file_lines.push("- Recent file excerpts:".to_string());
            file_lines.extend(file_excerpts);
        }
        let skipped_files = read_files
            .iter()
            .filter(|path| preserved_read_files.contains(*path))
            .take(3)
            .cloned()
            .collect::<Vec<_>>();
        if let Some(summary) = summarize_string_entries(&skipped_files, 3) {
            file_lines.push(format!(
                "- Skipped excerpts already preserved in tail: {}",
                summary
            ));
        }
        if file_lines.len() == 1 {
            file_lines.push("- No recent file context to restore.".to_string());
        }

        let mut plan_lines = vec![POST_COMPACT_PLAN_PREFIX.to_string()];
        plan_lines.push(format!(
            "- Plan mode: {}",
            if plan_snapshot.mode_enabled {
                "enabled"
            } else {
                "disabled"
            }
        ));
        plan_lines.push(format!(
            "- Permission mode: {}",
            plan_snapshot.permission_mode
        ));
        plan_lines.push(format!(
            "- Active plan file: {}",
            plan_snapshot
                .active_plan_file_path
                .as_deref()
                .unwrap_or("none")
        ));
        if plan_snapshot.mode_enabled {
            plan_lines.push(
                "- Restore contract: remain in read-only planning until the plan is approved."
                    .to_string(),
            );
        } else {
            plan_lines.push(
                "- Restore contract: no active plan mode; continue normal execution.".to_string(),
            );
        }

        let task_lines = render_task_restore_lines(self.runtime_tasks_snapshot());

        let mut tool_lines = vec![POST_COMPACT_TOOLS_PREFIX.to_string()];
        tool_lines.push(format!(
            "- Tool pool: {} active visible, {} active hidden, {} deferred visible, search={} (reason: {})",
            tool_pool.visible_active_count(),
            tool_pool.hidden_active_count(),
            tool_pool.visible_deferred_count(),
            if tool_pool.tool_search_enabled { "enabled" } else { "disabled" },
            tool_pool.tool_search_reason.as_deref().unwrap_or("none")
        ));
        tool_lines.push(format!(
            "- Tool inventory: total={} active={} deferred={} activations={} last={}",
            inventory.total_count,
            inventory.active_count,
            inventory.deferred_count,
            inventory.activation_count,
            inventory.last_activated_tool.as_deref().unwrap_or("none")
        ));

        let cache = &self.prompt_cache_runtime;
        let prompt_cache_lines = [
            POST_COMPACT_PROMPT_CACHE_PREFIX.to_string(),
            format!(
                "- Last turn: prompt={} completion={} write={} read={} edit_del={}",
                prompt_cache_value(cache.last_turn_prompt_tokens),
                prompt_cache_value(cache.last_turn_completion_tokens),
                prompt_cache_value(cache.last_turn_cache_write_tokens),
                prompt_cache_value(cache.last_turn_cache_read_tokens),
                prompt_cache_value(cache.last_turn_cache_edit_deletions)
            ),
            format!(
                "- Totals: turns={} write={} read={} edit_deletions={} deleted_tokens={}",
                cache.reported_turns,
                cache.cache_write_tokens_total,
                cache.cache_read_tokens_total,
                cache.cache_edit_deletions_total,
                cache.cache_deleted_tokens_total
            ),
            format!(
                "- Active cache edits: pending={} pinned={}",
                self.pending_cache_edit_refs.len(),
                self.pinned_cache_edit_refs.len()
            ),
            format!("- Expected next drop: compaction_{}", mode.label()),
            format!(
                "- Last break: count={} reason={} at={}",
                cache.prompt_cache_break_count,
                cache
                    .last_prompt_cache_break_reason
                    .as_deref()
                    .unwrap_or("none"),
                cache
                    .last_prompt_cache_break_at
                    .as_deref()
                    .unwrap_or("none")
            ),
            format!(
                "- Last transition: kind={} reason={} change={}",
                cache
                    .last_prompt_cache_transition_kind
                    .as_deref()
                    .unwrap_or("none"),
                cache
                    .last_prompt_cache_transition_reason
                    .as_deref()
                    .unwrap_or("none"),
                cache
                    .last_prompt_cache_change_summary
                    .as_deref()
                    .unwrap_or("none")
            ),
            format!(
                "- Last hashes: prefix={} system={} restore={} tool={} message={}",
                prompt_cache_text_value(self.last_prompt_cache_prefix_hash.as_ref()),
                prompt_cache_text_value(self.last_prompt_cache_system_hash.as_ref()),
                prompt_cache_text_value(self.last_prompt_cache_restore_hash.as_ref()),
                prompt_cache_text_value(self.last_prompt_cache_tool_hash.as_ref()),
                prompt_cache_text_value(self.last_prompt_cache_message_hash.as_ref())
            ),
        ];

        let mut mcp_lines = vec![POST_COMPACT_MCP_PREFIX.to_string()];
        mcp_lines.push(format!(
            "- MCP: visible_tools={} deferred_tools={} cache(list {} hit/{} miss, read {} hit/{} miss)",
            tool_pool.visible_mcp_count(),
            inventory.mcp_deferred_count,
            mcp_cache.list_hits,
            mcp_cache.list_misses,
            mcp_cache.read_hits,
            mcp_cache.read_misses
        ));

        let mut skill_lines = vec![POST_COMPACT_SKILLS_PREFIX.to_string()];
        if !active_skills.is_empty() {
            let rendered = active_skills
                .iter()
                .take(5)
                .map(|skill| {
                    if skill.metadata.paths.is_empty() {
                        skill.name.clone()
                    } else {
                        format!(
                            "{} (paths: {})",
                            skill.name,
                            skill.metadata.paths.join(", ")
                        )
                    }
                })
                .collect::<Vec<_>>()
                .join("; ");
            skill_lines.push(format!("- Path-gated active skills: {}", rendered));
        }
        skill_lines.extend(render_skill_invocation_restore_lines(&skill_invocations));
        if !skill_names.is_empty() {
            skill_lines.push(format!("- Available skills: {}", skill_names.join(", ")));
        } else {
            skill_lines.push("- No skills discovered.".to_string());
        }

        let mut artifact_lines = vec![POST_COMPACT_ARTIFACTS_PREFIX.to_string()];
        if let Some(path) = session_memory_path {
            artifact_lines.push(format!(
                "- Session memory artifact: {}",
                display_compaction_memory_path(&project_root, path)
            ));
        }
        if let Some(path) = transcript_path {
            artifact_lines.push(format!(
                "- Compaction transcript: {}",
                display_compaction_memory_path(&project_root, path)
            ));
        }
        if let Some(path) = self.last_tool_turn_artifact_path.as_deref() {
            artifact_lines.push(format!("- Latest tool artifact: {}", path));
        }
        if let Some(path) = self.last_turn_artifact_path.as_deref() {
            artifact_lines.push(format!("- Latest turn artifact: {}", path));
        }
        if artifact_lines.len() == 1 {
            artifact_lines.push("- No artifact links available.".to_string());
        }

        let blocks = vec![
            (RestoreBlockKind::Runtime, runtime_lines.join("\n")),
            (RestoreBlockKind::Files, file_lines.join("\n")),
            (RestoreBlockKind::Plan, plan_lines.join("\n")),
            (RestoreBlockKind::Tasks, task_lines.join("\n")),
            (RestoreBlockKind::Tools, tool_lines.join("\n")),
            (RestoreBlockKind::PromptCache, prompt_cache_lines.join("\n")),
            (RestoreBlockKind::Skills, skill_lines.join("\n")),
            (RestoreBlockKind::Mcp, mcp_lines.join("\n")),
            (RestoreBlockKind::Artifacts, artifact_lines.join("\n")),
        ];
        apply_restore_budget(blocks)
    }

    async fn finalize_compaction_result(
        &mut self,
        mode: CompactionMode,
        prompt_tokens: u32,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
        pre_compact_messages: Vec<Message>,
        report: CompressionReport,
        used_session_memory: bool,
    ) -> bool {
        let mode_label = mode.label();
        let mut report = report;

        if !used_session_memory && report.removed > 0 {
            if let Some(summary) = self
                .generate_structured_compaction_summary(
                    &report.removed_messages,
                    self.last_turn_artifact_path.as_deref(),
                    CompactionSummaryScope::Full,
                )
                .await
            {
                let previous_summary = report.summary.clone();
                self.replace_compaction_summary_message(previous_summary.as_deref(), &summary);
                report.summary = Some(summary);
            }
        }

        let mut session_memory_path = None;
        let mut transcript_path = None;
        let project_root = self.context.working_dir_compat();
        let compacted_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        match persist_compaction_memory_async(
            &project_root,
            &self.context.session_id,
            &report,
            &self.files_read,
            &self.files_modified,
        )
        .await
        {
            Ok(path) => {
                session_memory_path = Some(path);
            }
            Err(err) => warn!("Failed to persist session memory after compaction: {}", err),
        }
        let post_compact_estimated_tokens = self
            .context_manager
            .estimate_tokens_for_messages(&self.messages);
        let auto_compact_threshold = self.context_manager.compression_threshold_tokens();
        let will_retrigger_next_turn = post_compact_estimated_tokens >= auto_compact_threshold;
        self.last_post_compaction_estimated_tokens = Some(post_compact_estimated_tokens as u32);
        self.last_post_compaction_threshold_tokens = Some(auto_compact_threshold as u32);
        self.last_post_compaction_will_retrigger = Some(will_retrigger_next_turn);

        let mut compact_boundary = CompactBoundaryRuntimeState {
            mode: mode_label.to_string(),
            timestamp: compacted_at.clone(),
            removed_count: report.removed,
            tool_results_truncated: report.tool_results_truncated,
            preserved_tail_range: preserved_tail_range(&pre_compact_messages, &self.messages),
            summary_fingerprint: compact_summary_fingerprint(report.summary.as_ref()),
            post_compact_estimated_tokens: post_compact_estimated_tokens as u32,
            post_compact_threshold_tokens: auto_compact_threshold as u32,
            post_compact_token_delta: post_compact_estimated_tokens as i64
                - auto_compact_threshold as i64,
            will_retrigger_next_turn,
            artifact_paths: Vec::new(),
        };
        push_artifact_path(
            &mut compact_boundary.artifact_paths,
            session_memory_path.as_deref(),
        );

        match write_compaction_transcript_async(
            &project_root,
            &self.context.session_id,
            &pre_compact_messages,
            &report,
            mode_label,
            &self.failed_tool_call_ids,
            session_memory_path.as_deref(),
            &self.files_read,
            &self.files_modified,
            Some(&compact_boundary),
        )
        .await
        {
            Ok(path) => {
                push_artifact_path(&mut compact_boundary.artifact_paths, Some(&path));
                transcript_path = Some(path);
            }
            Err(err) => warn!("Failed to write compaction transcript: {}", err),
        }

        let (restore_messages, restore_budget) = self
            .build_post_compact_restore_messages(
                mode,
                session_memory_path.as_deref(),
                transcript_path.as_deref(),
                Some(post_compact_estimated_tokens as u32),
                Some(auto_compact_threshold as u32),
                Some(will_retrigger_next_turn),
            )
            .await;
        self.last_restore_budget = Some(restore_budget.clone());
        let previous_restore_messages =
            load_post_compact_restore_state_artifact_async(&project_root, &self.context.session_id)
                .await;
        let restore_artifact_path = write_post_compact_restore_artifact_async(
            &project_root,
            &self.context.session_id,
            mode_label,
            &restore_messages,
            Some(&compact_boundary),
            Some(&restore_budget),
        )
        .await;
        let restore_state_artifact_path = write_post_compact_restore_state_artifact_async(
            &project_root,
            &self.context.session_id,
            mode_label,
            &restore_messages,
            Some(&compact_boundary),
            Some(&restore_budget),
        )
        .await;
        push_artifact_path(
            &mut compact_boundary.artifact_paths,
            restore_artifact_path.as_deref(),
        );
        push_artifact_path(
            &mut compact_boundary.artifact_paths,
            restore_state_artifact_path.as_deref(),
        );
        if let Some(previous) = previous_restore_messages.as_ref() {
            let _ = write_post_compact_restore_diff_artifact_async(
                &project_root,
                &self.context.session_id,
                previous,
                &restore_messages,
            )
            .await;
        }
        self.set_post_compact_restore_blocks(restore_messages);
        self.take_post_compact_restore_messages_from_conversation();
        self.clear_cache_edit_tracking();
        self.sync_persisted_messages_snapshot();

        let post_context = self.build_compaction_hook_context(
            HookEvent::PostCompact,
            mode_label,
            prompt_tokens,
            Some(&report),
            session_memory_path.as_deref(),
            transcript_path.as_deref(),
        );
        self.execute_advisory_hooks(HookEvent::PostCompact, post_context)
            .await;
        let compressed_context = self.build_compaction_hook_context(
            HookEvent::ContextCompressed,
            mode_label,
            prompt_tokens,
            Some(&report),
            session_memory_path.as_deref(),
            transcript_path.as_deref(),
        );
        self.execute_advisory_hooks(HookEvent::ContextCompressed, compressed_context)
            .await;

        let still_above_threshold = self
            .context_manager
            .exceeds_threshold_estimate(&self.messages);
        self.compaction_in_progress = false;

        if still_above_threshold && mode.is_auto() {
            self.record_compaction_cause("failed_above_threshold");
            self.record_compaction_failure(
                "context remains above the safety threshold after compaction",
                event_tx,
            );
        } else if mode.is_auto() {
            self.compaction_failures = 0;
        }

        let session_memory_path_str = session_memory_path
            .as_ref()
            .map(|p| p.display().to_string());
        let transcript_path_str = transcript_path.as_ref().map(|p| p.display().to_string());

        let _ = event_tx.send(EngineEvent::ContextCompressed {
            mode: mode_label.to_string(),
            removed: report.removed,
            tool_results_truncated: report.tool_results_truncated,
            summary: report.summary.clone(),
            session_memory_path: session_memory_path_str.clone(),
            transcript_path: transcript_path_str.clone(),
        });
        self.last_compaction_mode = Some(mode_label.to_string());
        self.last_compaction_at = Some(compacted_at);
        self.last_compaction_summary_excerpt = report.summary.as_ref().map(|summary| {
            let excerpt: String = summary.chars().take(160).collect();
            if summary.chars().count() > 160 {
                format!("{}...", excerpt)
            } else {
                excerpt
            }
        });
        self.last_compaction_session_memory_path = session_memory_path_str;
        self.last_compaction_transcript_path = transcript_path_str;
        self.last_compact_boundary = Some(compact_boundary);
        self.last_compaction_prompt_tokens = Some(prompt_tokens);
        self.compaction_prompt_tokens_total = self
            .compaction_prompt_tokens_total
            .saturating_add(prompt_tokens as u64);
        self.compaction_prompt_token_samples =
            self.compaction_prompt_token_samples.saturating_add(1);
        self.total_compactions = self.total_compactions.saturating_add(1);
        match mode {
            CompactionMode::Auto => {
                self.auto_compactions = self.auto_compactions.saturating_add(1);
                if used_session_memory {
                    self.record_compaction_cause("success_auto_session_memory");
                } else {
                    self.record_compaction_cause("success_auto");
                }
            }
            CompactionMode::Manual => {
                self.manual_compactions = self.manual_compactions.saturating_add(1);
                self.record_compaction_cause("success_manual");
            }
            CompactionMode::Reactive => {
                self.record_compaction_cause("success_reactive");
            }
        }
        self.set_expected_prompt_cache_drop_reason(format!("compaction_{}", mode_label));
        self.persist_session_artifacts();
        true
    }

    pub(super) async fn compact_context(
        &mut self,
        prompt_tokens: u32,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
        mode: CompactionMode,
        keep_last_override: Option<usize>,
    ) -> bool {
        let mode_label = mode.label();

        if mode.is_auto() && !self.current_query_source.allows_auto_compaction() {
            self.record_compaction_cause("skipped_query_source");
            debug!(
                "Skipping auto-compaction for query source {:?}",
                self.current_query_source
            );
            return false;
        }

        if mode.is_auto() && self.autocompact_disabled {
            self.record_compaction_cause("skipped_breaker_open");
            debug!("Skipping auto-compaction because the circuit breaker is open");
            return false;
        }

        if self.compaction_in_progress {
            self.record_compaction_cause("skipped_nested");
            warn!("Skipping nested compaction attempt");
            return false;
        }

        if mode.is_auto()
            && !self
                .context_manager
                .should_compress(prompt_tokens, &self.messages)
        {
            self.record_compaction_cause("skipped_below_threshold");
            return false;
        }

        self.compaction_in_progress = true;
        let _ = event_tx.send(EngineEvent::ContextCompactionStarted {
            mode: mode_label.to_string(),
        });

        let pre_context = self.build_compaction_hook_context(
            HookEvent::PreCompact,
            mode_label,
            prompt_tokens,
            None,
            None,
            None,
        );
        self.execute_advisory_hooks(HookEvent::PreCompact, pre_context)
            .await;

        let pre_compact_messages = self.messages.clone();
        let (report, used_session_memory) = if let Some(keep_last) = keep_last_override {
            (
                self.context_manager.compress_with_keep_last(
                    &mut self.messages,
                    keep_last,
                    self.last_turn_artifact_path.as_deref(),
                ),
                false,
            )
        } else if mode.is_auto() {
            if let Some(report) = self.try_session_memory_compaction() {
                self.record_compaction_cause("strategy_session_memory");
                (report, true)
            } else {
                (
                    self.context_manager.compress_with_turn_artifact(
                        &mut self.messages,
                        self.last_turn_artifact_path.as_deref(),
                    ),
                    false,
                )
            }
        } else {
            (
                self.context_manager.compress_with_turn_artifact(
                    &mut self.messages,
                    self.last_turn_artifact_path.as_deref(),
                ),
                false,
            )
        };
        if report.removed == 0 && report.tool_results_truncated == 0 {
            self.compaction_in_progress = false;
            match mode {
                CompactionMode::Auto => {
                    self.record_compaction_cause("failed_no_change");
                    self.record_compaction_failure("compression made no changes", event_tx);
                }
                CompactionMode::Reactive => {
                    self.record_compaction_cause("failed_reactive_no_change");
                }
                CompactionMode::Manual => {}
            }
            return false;
        }

        self.finalize_compaction_result(
            mode,
            prompt_tokens,
            event_tx,
            pre_compact_messages,
            report,
            used_session_memory,
        )
        .await
    }

    pub(super) fn estimated_prompt_tokens_for_current_messages(&self) -> u32 {
        let base = self
            .context_manager
            .estimate_tokens_for_messages(&self.messages);
        let restore = self
            .hidden_post_compact_restore_prompt_text()
            .map(|text| {
                self.context_manager
                    .estimate_tokens_for_messages(&[Message::system(text)])
            })
            .unwrap_or(0);
        base.saturating_add(restore).max(1) as u32
    }

    pub async fn force_compact(&mut self, event_tx: mpsc::UnboundedSender<EngineEvent>) -> bool {
        let estimated_tokens = self.estimated_prompt_tokens_for_current_messages();
        self.compact_context(estimated_tokens, &event_tx, CompactionMode::Manual, None)
            .await
    }

    pub async fn force_compact_keep_last(
        &mut self,
        keep_last: usize,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
    ) -> bool {
        let estimated_tokens = self.estimated_prompt_tokens_for_current_messages();
        self.compact_context(
            estimated_tokens,
            &event_tx,
            CompactionMode::Manual,
            Some(keep_last.max(1)),
        )
        .await
    }

    pub async fn force_partial_compact_up_to(
        &mut self,
        up_to: usize,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
    ) -> bool {
        let message_count = self.messages.len().saturating_sub(1);
        if message_count == 0 {
            return false;
        }
        let end = 1 + up_to.min(message_count);
        self.partial_compact_range(1, end, &event_tx).await
    }

    pub async fn force_partial_compact_from(
        &mut self,
        from: usize,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
    ) -> bool {
        let message_count = self.messages.len().saturating_sub(1);
        if message_count == 0 {
            return false;
        }
        let logical_start = from.clamp(1, message_count);
        let start = logical_start;
        self.partial_compact_range(start, self.messages.len(), &event_tx)
            .await
    }

    fn try_session_memory_compaction(&mut self) -> Option<CompressionReport> {
        let project_root = self.context.working_dir_compat();
        let (path, excerpt) = best_compaction_memory_excerpt(&project_root, 900)?;
        let summary = build_session_memory_compaction_summary(&project_root, &path, &excerpt);
        let report =
            self.context_manager
                .compact_with_external_summary(&mut self.messages, 8, summary);
        (report.removed > 0 || report.tool_results_truncated > 0).then_some(report)
    }

    fn expand_partial_compaction_range(
        &self,
        mut start: usize,
        mut end: usize,
    ) -> Option<(usize, usize)> {
        if start < 1 || end > self.messages.len() || start >= end {
            return None;
        }

        loop {
            let range = &self.messages[start..end];
            let summarized_tool_calls = collect_assistant_tool_call_ids(range);
            let summarized_tool_results = collect_tool_result_ids(range);
            let mut changed = false;

            let mut idx = 1;
            while idx < start {
                let message = &self.messages[idx];
                if matches!(message.role, Role::Assistant)
                    && message
                        .tool_calls
                        .iter()
                        .any(|call| summarized_tool_results.contains(&call.id))
                {
                    start = idx;
                    changed = true;
                    break;
                }
                idx += 1;
            }

            let mut idx = end;
            while idx < self.messages.len() {
                let message = &self.messages[idx];
                if matches!(message.role, Role::Tool)
                    && message
                        .tool_call_id
                        .as_ref()
                        .is_some_and(|id| summarized_tool_calls.contains(id))
                {
                    end = idx + 1;
                    changed = true;
                }
                idx += 1;
            }

            if !changed {
                return Some((start, end));
            }
        }
    }

    async fn partial_compact_range(
        &mut self,
        start: usize,
        end: usize,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) -> bool {
        if self.compaction_in_progress {
            self.record_compaction_cause("skipped_nested");
            return false;
        }

        let Some((start, end)) = self.expand_partial_compaction_range(start, end) else {
            return false;
        };
        if start >= end || end > self.messages.len() {
            return false;
        }

        self.compaction_in_progress = true;
        let prompt_tokens = self.estimated_prompt_tokens_for_current_messages();
        let _ = event_tx.send(EngineEvent::ContextCompactionStarted {
            mode: CompactionMode::Manual.label().to_string(),
        });
        let pre_context = self.build_compaction_hook_context(
            HookEvent::PreCompact,
            CompactionMode::Manual.label(),
            prompt_tokens,
            None,
            None,
            None,
        );
        self.execute_advisory_hooks(HookEvent::PreCompact, pre_context)
            .await;

        let pre_compact_messages = self.messages.clone();
        let removed_messages = self.messages[start..end].to_vec();
        if removed_messages.is_empty() {
            self.compaction_in_progress = false;
            return false;
        }

        let summary = self
            .generate_structured_compaction_summary(
                &removed_messages,
                self.last_turn_artifact_path.as_deref(),
                if start <= 1 {
                    CompactionSummaryScope::PartialUpTo
                } else {
                    CompactionSummaryScope::PartialFrom
                },
            )
            .await
            .unwrap_or_else(|| {
                build_fallback_compaction_summary(
                    &removed_messages,
                    self.last_turn_artifact_path.as_deref(),
                )
            });

        self.messages.drain(start..end);
        self.messages
            .insert(start, Message::system(summary.clone()));

        let report = CompressionReport {
            removed: removed_messages.len(),
            tool_results_truncated: 0,
            summary: Some(summary),
            removed_messages,
        };

        self.finalize_compaction_result(
            CompactionMode::Manual,
            prompt_tokens,
            event_tx,
            pre_compact_messages,
            report,
            false,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use yode_llm::types::Message;

    use super::blocks::RestoreBlockKind;
    use super::budget::apply_restore_budget;
    use super::summarizer::{
        format_llm_compaction_summary_content, parse_prompt_too_long_token_gap,
        truncate_head_for_summary_retry,
    };

    #[test]
    fn parses_prompt_too_long_gap_from_error_text() {
        assert_eq!(
            parse_prompt_too_long_token_gap("prompt is too long: 137500 tokens > 135000 maximum"),
            Some(2500)
        );
        assert_eq!(parse_prompt_too_long_token_gap("something else"), None);
    }

    #[test]
    fn truncate_head_retry_prefers_reported_token_gap() {
        let messages = vec![
            Message::user("x".repeat(8_000)),
            Message::assistant("y".repeat(8_000)),
            Message::user("keep"),
        ];

        let truncated = truncate_head_for_summary_retry(
            &messages,
            "prompt is too long: 6000 tokens > 2000 maximum",
        );

        assert!(truncated.len() < messages.len());
        assert_eq!(
            truncated
                .last()
                .and_then(|message| message.content.as_deref()),
            Some("keep")
        );
    }

    #[test]
    fn formats_llm_compaction_summary_by_stripping_analysis() {
        let raw = "<analysis>\nprivate draft\n</analysis>\n\n<summary>\n## Goals\n- Continue compact parity\n\n\n## Next Steps\n- Run tests\n</summary>";

        let formatted = format_llm_compaction_summary_content(raw).unwrap();

        assert!(!formatted.contains("private draft"));
        assert!(!formatted.contains("<summary>"));
        assert!(formatted.starts_with("## Goals"));
        assert!(formatted.contains("## Next Steps"));
    }

    #[test]
    fn restore_budget_truncates_large_blocks_with_recovery_hint() {
        let oversized_files = format!(
            "[Post-compact restore: files]\n- Recent file excerpts:\n{}",
            "very large file excerpt ".repeat(2_000)
        );
        let (blocks, budget) =
            apply_restore_budget(vec![(RestoreBlockKind::Files, oversized_files)]);

        assert!(budget.used_tokens <= budget.total_tokens);
        assert_eq!(budget.entries.len(), 1);
        assert!(budget.entries[0].truncated);
        let files_block = &blocks[0].1;
        assert!(files_block.contains("Restore budget: truncated"));
        assert!(files_block.contains("Re-read the named files"));
    }
}
