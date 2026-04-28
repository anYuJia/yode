#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/289-eighth-handoff-artifact.md}"

cat >"$out_file" <<'EOF'
# Eighth Handoff Artifact

- ci: `.github/workflows/ci.yml`
- replay: `scripts/parity-replay-serialize.sh`
- artifact bundle: `scripts/parity-artifact-bundle.sh`
- next tracker: `docs/optimization/292-ninth-100-claude-output-parity-tracker.md`
EOF

echo "Eighth handoff artifact written: $out_file"
