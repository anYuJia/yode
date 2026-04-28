#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/295-eighth-count-note.md}"
count="$(grep -Ec '^- `\[x\]` [0-9][0-9][0-9]' docs/optimization/256-eighth-100-claude-output-parity-tracker.md || true)"

cat >"$out_file" <<EOF
# Eighth Count Note

- completed_rows: $count
EOF

echo "Eighth count note written: $out_file"
