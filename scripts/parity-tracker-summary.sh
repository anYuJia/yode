#!/usr/bin/env bash
set -euo pipefail

TRACKER="${1:-docs/optimization/36-next-100-step-claude-code-parity-tracker.md}"

if [[ ! -f "$TRACKER" ]]; then
  echo "Tracker not found: $TRACKER" >&2
  exit 1
fi

done_count=$(grep -c '^-[[:space:]]\+`\[x\][[:space:]][0-9]' "$TRACKER" || true)
in_progress_count=$(grep -c '^-[[:space:]]\+`\[~\][[:space:]][0-9]' "$TRACKER" || true)
todo_count=$(grep -c '^-[[:space:]]\+`\[ \][[:space:]][0-9]' "$TRACKER" || true)

echo "Tracker: $TRACKER"
echo "Done: $done_count"
echo "In progress: $in_progress_count"
echo "Todo: $todo_count"
echo
echo "Current focus:"
awk '
  /^当前焦点：/ { capture=1; next }
  capture && /^- `/ { print "  " $0; exit }
' "$TRACKER"
echo
echo "Next 10 open items:"
awk '
  /^- `\[ \] [0-9]/ { print; count++; if (count == 10) exit }
' "$TRACKER"
