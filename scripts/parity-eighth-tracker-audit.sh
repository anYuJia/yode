#!/usr/bin/env bash
set -euo pipefail

done_count="$(grep -Ec '^- `\[x\]` [0-9][0-9][0-9]' docs/optimization/256-eighth-100-claude-output-parity-tracker.md || true)"
echo "done=$done_count"
if [[ "$done_count" != "100" ]]; then
  echo "Eighth tracker not complete" >&2
  exit 1
fi

echo "Eighth tracker audit ok"
