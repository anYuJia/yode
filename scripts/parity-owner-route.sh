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
  *hook*|*task*|*recovery*)
    owner="hooks-tasks"
    next="cargo test -p yode-tui task_summary_uses_monitor_headline --quiet"
    ;;
  *inspect*|*confirm*)
    owner="inspector-confirm"
    next="cargo test -p yode-tui confirmation_density_switches_on_narrow_widths --quiet"
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
