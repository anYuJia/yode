#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/260-sixth-parity-signoff.md}"

bash scripts/parity-final-count-audit.sh >/dev/null
bash scripts/parity-final-docs-audit.sh >/dev/null
bash scripts/parity-final-test-audit.sh >/dev/null
bash scripts/parity-final-automation-audit.sh >/dev/null

cat >"$out_file" <<'EOF'
# Sixth Parity Signoff

- counts: verified
- docs: verified
- tests: verified
- automation: verified
- next tracker: `docs/optimization/256-eighth-100-claude-output-parity-tracker.md`
EOF

echo "Parity signoff written: $out_file"
