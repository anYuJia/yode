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
    next="cargo test -p yode-core engine::tests::stream_recovery::truncated_stream_tool_calls_are_discarded_not_executed --quiet"
    ;;
  *markdown*|*cjk*|*table*|*heading*|*code*)
    owner="markdown-rendering"
    next="cargo test -p yode-core permission::tests::test_auto_mode_bash_classification --quiet"
    ;;
  *remote*|*workflow*)
    owner="remote-workflow"
    next="cargo test -p yode-core permission::tests::test_rule_priority --quiet"
    ;;
  *doctor*|*export*|*bundle*)
    owner="doctor-export"
    next="cargo test -p yode-core db::tests::open_migrates_legacy_database_columns --quiet"
    ;;
  *permission*)
    owner="permissions"
    next="cargo test -p yode-core permission::tests::test_managed_deny_takes_precedence_over_user_allow --quiet"
    ;;
  *prompt*|*cache*)
    owner="prompt-cache"
    next="cargo test -p yode-core cost_tracker::tests:: --quiet"
    ;;
  *status*|*diagnostics*)
    owner="status-diagnostics"
    next="cargo test -p yode-core engine::tests::stream_recovery::completed_stream_tool_calls_execute_normally --quiet"
    ;;
  *review*)
    owner="review-artifacts"
    next="cargo test -p yode-core db::tests::open_enables_pragmas_and_sets_schema_version --quiet"
    ;;
  *hook*|*task*|*recovery*)
    owner="hooks-tasks"
    next="cargo test -p yode-core task_summary_uses_monitor_headline --quiet"
    ;;
  *inspect*|*confirm*)
    owner="inspector-confirm"
    next="cargo test -p yode-core confirmation_density_switches_on_narrow_widths --quiet"
    ;;
  *artifact*)
    owner="artifact-nav"
    next="cargo test -p yode-core artifact_inspector_applies_badges --quiet"
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
