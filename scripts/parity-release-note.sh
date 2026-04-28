#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/245-parity-release-note-draft.md}"

fourth_done="$(grep -Ec '^- `\[x\]`? [0-9][0-9][0-9]' docs/optimization/236-fourth-100-claude-output-parity-tracker.md || true)"
fifth_done="$(grep -Ec '^- `\[x\]`? [0-9][0-9][0-9]' docs/optimization/238-fifth-100-claude-output-parity-tracker.md || true)"
sixth_done="$(grep -Ec '^- `\[x\]`? [0-9][0-9][0-9]' docs/optimization/240-sixth-100-claude-output-parity-tracker.md || true)"

cat >"$out_file" <<EOF
# Parity Release Note Draft

## Status

- Fourth tracker: ${fourth_done}/100
- Fifth tracker: ${fifth_done}/100
- Sixth tracker: ${sixth_done}/100

## Highlights

- Snapshot CI, replay CI, visual CI, docs CI, fixture freshness, retention audit, and local wrapper now exist as executable scripts.
- Manifest commands are audited against real \`yode-tui\` tests, scripts, and handoff files.
- Snapshot baseline drift is stabilized through deterministic regression output and a reusable visual diff tool.
- Golden snapshot storage and fixture scaffolding are now available for future automation rounds.
EOF

echo "Parity release note written: $out_file"
