#!/usr/bin/env bash
set -euo pipefail

docs=(
  docs/optimization/241-parity-ci-automation-guide.md
  docs/optimization/242-golden-snapshot-storage-proposal.md
  docs/optimization/243-parity-risk-register.md
  docs/optimization/244-parity-known-limitations.md
  docs/optimization/245-parity-release-note-draft.md
  docs/optimization/246-seventh-100-claude-output-parity-tracker.md
)

for doc in "${docs[@]}"; do
  [[ -f "$doc" ]] || { echo "Doc missing: $doc" >&2; exit 1; }
done

rg -q 'parity-baseline-refresh.sh' docs/optimization/241-parity-ci-automation-guide.md
rg -q 'parity-benchmark-ci.sh' docs/optimization/241-parity-ci-automation-guide.md
rg -q 'parity-golden-store-ci.sh' docs/optimization/241-parity-ci-automation-guide.md
rg -q 'parity-visual-diff.sh' docs/optimization/242-golden-snapshot-storage-proposal.md

echo "Parity doc link audit ok"
