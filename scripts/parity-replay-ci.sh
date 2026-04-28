#!/usr/bin/env bash
set -euo pipefail

tests=(
  latest_focus_mixed_tool_system_and_error_runs
  cjk_tables_render_without_losing_cells
  grouped_system_entries_names_remote_review_and_workflow_batches
  build_runtime_timeline_merges_dated_state_and_artifact_events
  inspector_internal_actions_resolve_pending_confirmation
  grouped_subagent_batch_compacts_multiple_segments
  ask_user_entries_render_question_framing
  print_export_regression_snapshot
  rule_and_suggestion_helpers_render
  transcript_picker_includes_folded_summary_preview
  render_latest_transcript_surfaces_workspace_sections
)

for test_name in "${tests[@]}"; do
  cargo test -p yode-tui "$test_name" --quiet
done

echo "Parity replay CI ok"
