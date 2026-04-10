use super::*;

impl AgentEngine {
    pub(super) async fn maybe_compact_context(
        &mut self,
        prompt_tokens: u32,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) {
        let _ = self.compact_context(prompt_tokens, event_tx, false).await;
    }

    pub(super) async fn compact_context(
        &mut self,
        prompt_tokens: u32,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
        force: bool,
    ) -> bool {
        let mode = if force { "manual" } else { "auto" };

        if !force && !self.current_query_source.allows_auto_compaction() {
            self.record_compaction_cause("skipped_query_source");
            debug!(
                "Skipping auto-compaction for query source {:?}",
                self.current_query_source
            );
            return false;
        }

        if !force && self.autocompact_disabled {
            self.record_compaction_cause("skipped_breaker_open");
            debug!("Skipping auto-compaction because the circuit breaker is open");
            return false;
        }

        if self.compaction_in_progress {
            self.record_compaction_cause("skipped_nested");
            warn!("Skipping nested auto-compaction attempt");
            return false;
        }

        if !force
            && !self
                .context_manager
                .should_compress(prompt_tokens, &self.messages)
        {
            self.record_compaction_cause("skipped_below_threshold");
            return false;
        }

        self.compaction_in_progress = true;

        let pre_context = self.build_compaction_hook_context(
            HookEvent::PreCompact,
            mode,
            prompt_tokens,
            None,
            None,
            None,
        );
        self.execute_advisory_hooks(HookEvent::PreCompact, pre_context)
            .await;

        let pre_compact_messages = self.messages.clone();
        let report = self.context_manager.compress_with_report(&mut self.messages);
        if report.removed == 0 && report.tool_results_truncated == 0 {
            self.compaction_in_progress = false;
            self.record_compaction_cause("failed_no_change");
            if !force {
                self.record_compaction_failure("compression made no changes", event_tx);
            }
            return false;
        }

        let mut session_memory_path = None;
        let mut transcript_path = None;
        let project_root = self.context.working_dir_compat();
        match persist_compaction_memory(
            &project_root,
            &self.context.session_id,
            &report,
            &self.files_read,
            &self.files_modified,
        ) {
            Ok(path) => {
                session_memory_path = Some(path);
                self.rebuild_system_prompt();
            }
            Err(err) => warn!("Failed to persist session memory after compaction: {}", err),
        }
        match write_compaction_transcript(
            &project_root,
            &self.context.session_id,
            &pre_compact_messages,
            &report,
            mode,
            &self.failed_tool_call_ids,
            session_memory_path.as_deref(),
            &self.files_read,
            &self.files_modified,
        ) {
            Ok(path) => transcript_path = Some(path),
            Err(err) => warn!("Failed to write compaction transcript: {}", err),
        }
        self.sync_persisted_messages_snapshot();

        let post_context = self.build_compaction_hook_context(
            HookEvent::PostCompact,
            mode,
            prompt_tokens,
            Some(&report),
            session_memory_path.as_deref(),
            transcript_path.as_deref(),
        );
        self.execute_advisory_hooks(HookEvent::PostCompact, post_context)
            .await;
        let compressed_context = self.build_compaction_hook_context(
            HookEvent::ContextCompressed,
            mode,
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

        if still_above_threshold && !force {
            self.record_compaction_cause("failed_above_threshold");
            self.record_compaction_failure(
                "context remains above the safety threshold after compaction",
                event_tx,
            );
        } else {
            self.compaction_failures = 0;
        }

        let session_memory_path_str = session_memory_path
            .as_ref()
            .map(|p| p.display().to_string());
        let transcript_path_str = transcript_path.as_ref().map(|p| p.display().to_string());

        let _ = event_tx.send(EngineEvent::ContextCompressed {
            mode: mode.to_string(),
            removed: report.removed,
            tool_results_truncated: report.tool_results_truncated,
            summary: report.summary.clone(),
            session_memory_path: session_memory_path_str.clone(),
            transcript_path: transcript_path_str.clone(),
        });
        self.last_compaction_mode = Some(mode.to_string());
        self.last_compaction_at =
            Some(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
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
        self.last_compaction_prompt_tokens = Some(prompt_tokens);
        self.compaction_prompt_tokens_total = self
            .compaction_prompt_tokens_total
            .saturating_add(prompt_tokens as u64);
        self.compaction_prompt_token_samples =
            self.compaction_prompt_token_samples.saturating_add(1);
        self.total_compactions = self.total_compactions.saturating_add(1);
        if mode == "auto" {
            self.auto_compactions = self.auto_compactions.saturating_add(1);
            self.record_compaction_cause("success_auto");
        } else {
            self.manual_compactions = self.manual_compactions.saturating_add(1);
            self.record_compaction_cause("success_manual");
        }
        self.persist_session_artifacts();
        true
    }

    pub(super) fn estimated_prompt_tokens_for_current_messages(&self) -> u32 {
        let char_count: usize = self
            .messages
            .iter()
            .map(|m| {
                m.content.as_ref().map(|c| c.len()).unwrap_or(0)
                    + m.tool_calls
                        .iter()
                        .map(|tc| tc.arguments.len() + tc.name.len())
                        .sum::<usize>()
            })
            .sum();
        (char_count / 4).max(1) as u32
    }

    pub async fn force_compact(&mut self, event_tx: mpsc::UnboundedSender<EngineEvent>) -> bool {
        let estimated_tokens = self.estimated_prompt_tokens_for_current_messages();
        self.compact_context(estimated_tokens, &event_tx, true)
            .await
    }
}
