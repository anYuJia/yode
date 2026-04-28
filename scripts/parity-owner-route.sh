#!/usr/bin/env bash
set -euo pipefail

surface="${1:-}"

if [[ -z "$surface" ]]; then
  echo "Usage: $0 <surface-or-section>" >&2
  exit 1
fi

case "$surface" in
  *transcript*|*assistant*|*system*|*error*|*subagent*)
    owner="transcript-rendering"
    next="cargo test -p yode-tui latest_focus_mixed_tool_system_and_error_runs --quiet"
    ;;
  *markdown*|*cjk*|*table*|*heading*|*code*)
    owner="markdown-rendering"
    next="cargo test -p yode-tui chat_markdown --quiet"
    ;;
  *remote*|*workflow*)
    owner="remote-workflow"
    next="cargo test -p yode-tui workflow_preview_uses_dense_step_lines --quiet"
    ;;
  *doctor*|*export*|*bundle*)
    owner="doctor-export"
    next="cargo test -p yode-tui print_export_regression_snapshot --quiet"
    ;;
  *permission*)
    owner="permissions"
    next="cargo test -p yode-tui rule_and_suggestion_helpers_render --quiet"
    ;;
  *prompt*|*cache*)
    owner="prompt-cache"
    next="cargo test -p yode-tui prompt_cache_badge_shows_read_write_totals --quiet"
    ;;
  *status*|*diagnostics*)
    owner="status-diagnostics"
    next="cargo test -p yode-tui status_bar_density_compacts_on_narrow_widths --quiet"
    ;;
  *review*)
    owner="review-artifacts"
    next="cargo test -p yode-tui print_remote_bundle_regression_snapshot --quiet"
    ;;
  *hook*|*task*|*recovery*)
    owner="hooks-tasks"
    next="cargo test -p yode-tui task_summary_uses_monitor_headline --quiet"
    ;;
  *inspect*|*confirm*)
    owner="inspector-confirm"
    next="cargo test -p yode-tui confirmation_density_switches_on_narrow_widths --quiet"
    ;;
  *artifact*)
    owner="artifact-nav"
    next="cargo test -p yode-tui artifact_inspector_applies_badges --quiet"
    ;;
  *snapshot*|*catalog*)
    owner="snapshot-governance"
    next="bash scripts/parity-ci-dry-run.sh --skip-cargo"
    ;;
  *)
    owner="governance"
    next="bash scripts/parity-fixture-audit.sh"
    ;;
esac

echo "surface=$surface"
echo "owner=$owner"
echo "next=$next"
