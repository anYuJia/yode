#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/255-sixth-parity-handoff.md}"

cat >"$out_file" <<'EOF'
# Sixth Parity Handoff

- closeout audit: `scripts/parity-closeout-audit.sh`
- local CI: `scripts/parity-ci-local.sh`
- next tracker: `docs/optimization/256-eighth-100-claude-output-parity-tracker.md`
EOF

echo "Parity handoff artifact written: $out_file"
