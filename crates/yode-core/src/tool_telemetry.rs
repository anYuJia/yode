use std::collections::BTreeMap;

use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

use yode_llm::types::ToolCall;
use yode_tools::tool::ToolResult;

use crate::tool_runtime::{
    write_tool_turn_artifact, ToolResultTruncationView, ToolTurnArtifact,
};

use super::{
    hex_short, AgentEngine, EngineEvent, ToolExecutionTrace, TOOL_BUDGET_NOTICE,
    TOOL_BUDGET_WARNING,
};

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
    }

    pub(super) fn record_compaction_cause(&mut self, cause: &str) {
        *self
            .compaction_cause_histogram
            .entry(cause.to_string())
            .or_insert(0) += 1;
    }

    pub(super) fn record_response_usage(
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
            self.prompt_cache_runtime.last_turn_prompt_tokens = Some(usage.prompt_tokens);
            self.prompt_cache_runtime.last_turn_completion_tokens = Some(usage.completion_tokens);
            self.prompt_cache_runtime.last_turn_cache_write_tokens = Some(usage.cache_write_tokens);
            self.prompt_cache_runtime.last_turn_cache_read_tokens = Some(usage.cache_read_tokens);
            self.prompt_cache_runtime.reported_turns =
                self.prompt_cache_runtime.reported_turns.saturating_add(1);
            if usage.cache_write_tokens > 0 {
                self.prompt_cache_runtime.cache_write_turns =
                    self.prompt_cache_runtime.cache_write_turns.saturating_add(1);
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
        if self.tool_call_count >= TOOL_BUDGET_WARNING && !self.current_turn_budget_warning_emitted {
            let message =
                "Budget warning: 25 tool calls used. Stop exploring and produce your report.";
            self.current_turn_budget_warning_emitted = true;
            self.tool_budget_warning_count = self.tool_budget_warning_count.saturating_add(1);
            self.last_tool_budget_warning = Some(message.to_string());
            return Some(message.to_string());
        }

        if self.tool_call_count >= TOOL_BUDGET_NOTICE && !self.current_turn_budget_notice_emitted {
            let message =
                "Budget notice: 15 tool calls used. Consider summarizing current findings before continuing.";
            self.current_turn_budget_notice_emitted = true;
            self.tool_budget_notice_count = self.tool_budget_notice_count.saturating_add(1);
            self.last_tool_budget_warning = Some(message.to_string());
            return Some(message.to_string());
        }

        None
    }

    fn note_tool_truncation(&mut self, truncation: &ToolResultTruncationView) {
        self.tool_truncation_count = self.tool_truncation_count.saturating_add(1);
        self.current_turn_truncated_results = self.current_turn_truncated_results.saturating_add(1);
        self.last_tool_truncation_reason = Some(truncation.reason.clone());
    }

    fn summarize_result_metadata(metadata: &Option<Value>) -> Option<String> {
        let meta = metadata.as_ref()?.as_object()?;
        let mut parts = Vec::new();
        for key in [
            "file_path",
            "byte_count",
            "line_count",
            "replacements",
            "applied_edits",
            "command_type",
            "rewrite_suggestion",
            "url",
            "count",
        ] {
            if let Some(value) = meta.get(key) {
                let rendered = if let Some(s) = value.as_str() {
                    s.to_string()
                } else {
                    value.to_string()
                };
                parts.push(format!("{}={}", key, rendered));
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(", "))
        }
    }

    fn extract_diff_preview(metadata: &Option<Value>) -> Option<String> {
        let diff = metadata
            .as_ref()
            .and_then(|meta| meta.get("diff_preview"))
            .and_then(|value| value.as_object())?;

        let mut lines = Vec::new();
        if let Some(removed) = diff.get("removed").and_then(|value| value.as_array()) {
            for line in removed.iter().filter_map(|value| value.as_str()) {
                lines.push(format!("-{}", line));
            }
            if let Some(extra) = diff.get("more_removed").and_then(|value| value.as_u64()) {
                if extra > 0 {
                    lines.push(format!("... {} more removed", extra));
                }
            }
        }
        if let Some(added) = diff.get("added").and_then(|value| value.as_array()) {
            for line in added.iter().filter_map(|value| value.as_str()) {
                lines.push(format!("+{}", line));
            }
            if let Some(extra) = diff.get("more_added").and_then(|value| value.as_u64()) {
                if extra > 0 {
                    lines.push(format!("... {} more added", extra));
                }
            }
        }

        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }

    fn output_preview(content: &str) -> String {
        const MAX_LINES: usize = 6;
        const MAX_CHARS: usize = 500;

        let lines = content.lines().take(MAX_LINES).collect::<Vec<_>>();
        let mut preview = lines.join("\n");
        if preview.chars().count() > MAX_CHARS {
            preview = preview.chars().take(MAX_CHARS).collect::<String>();
            preview.push_str("\n... [preview truncated]");
        } else if content.lines().count() > MAX_LINES {
            preview.push_str("\n... [more lines omitted]");
        }
        preview
    }

    fn failure_signature(tool_call: &ToolCall, error_type: Option<&str>) -> String {
        let mut hasher = Sha256::new();
        hasher.update(tool_call.name.as_bytes());
        hasher.update(tool_call.arguments.as_bytes());
        if let Some(kind) = error_type {
            hasher.update(kind.as_bytes());
        }
        let digest = hasher.finalize();
        format!(
            "{}:{}:{}",
            tool_call.name,
            error_type.unwrap_or("unknown"),
            hex_short(&digest)
        )
    }

    fn tool_truncation_from_metadata(metadata: &Option<Value>) -> Option<ToolResultTruncationView> {
        let tool_runtime = metadata
            .as_ref()
            .and_then(|meta| meta.get("tool_runtime"))
            .and_then(|value| value.as_object())?;
        let truncation = tool_runtime.get("truncation")?.as_object()?;
        Some(ToolResultTruncationView {
            reason: truncation
                .get("reason")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown")
                .to_string(),
            original_bytes: truncation
                .get("original_bytes")
                .and_then(|value| value.as_u64())
                .unwrap_or(0) as usize,
            kept_bytes: truncation
                .get("kept_bytes")
                .and_then(|value| value.as_u64())
                .unwrap_or(0) as usize,
            omitted_bytes: truncation
                .get("omitted_bytes")
                .and_then(|value| value.as_u64())
                .unwrap_or(0) as usize,
        })
    }

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
            let signature = Self::failure_signature(tool_call, error_type.as_deref());
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

        let truncation = Self::tool_truncation_from_metadata(&result.metadata);
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
            metadata_summary: Self::summarize_result_metadata(&result.metadata),
            diff_preview: Self::extract_diff_preview(&result.metadata),
            output_preview: Self::output_preview(&result.content),
        };
        self.current_tool_execution_traces.push(trace);
    }

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
}
