#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/286-eighth-release-note-draft.md}"

cat >"$out_file" <<EOF
# Eighth Release Note Draft

- CI parity jobs wired into \`.github/workflows/ci.yml\`
- replay fixtures serialized into \`.yode/benchmarks/replay/\`
- parity artifact bundles include snapshot, replay, golden, diff, and docs outputs
- failure-route, candidate-compare, and catalog-compare reports are uploadable artifacts
EOF

echo "Eighth release note written: $out_file"
