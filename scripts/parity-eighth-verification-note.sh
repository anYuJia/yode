#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/296-eighth-verification-note.md}"

cat >"$out_file" <<'EOF'
# Eighth Verification Note

- `scripts/parity-docs-ci.sh`
- `scripts/parity-workflow-audit.sh`
- `scripts/parity-replay-hardening-audit.sh`
- `scripts/parity-artifact-hardening-audit.sh`
- `scripts/parity-ci-dry-run.sh`
- `scripts/parity-ci-local.sh`
EOF

echo "Eighth verification note written: $out_file"
