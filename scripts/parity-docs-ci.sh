#!/usr/bin/env bash
set -euo pipefail

bash scripts/parity-tracker-count.sh \
  docs/optimization/236-fourth-100-claude-output-parity-tracker.md 100 \
  docs/optimization/238-fifth-100-claude-output-parity-tracker.md 100 \
  docs/optimization/240-sixth-100-claude-output-parity-tracker.md 25 >/dev/null

bash scripts/parity-owner-enforcement.sh >/dev/null
bash scripts/parity-risk-register.sh >/dev/null
bash scripts/parity-limitations-ci.sh >/dev/null
bash scripts/parity-release-note.sh >/dev/null

required_docs=(
  docs/optimization/237-fourth-100-claude-output-closeout.md
  docs/optimization/239-fifth-100-claude-output-closeout.md
  docs/optimization/241-parity-ci-automation-guide.md
  docs/optimization/242-golden-snapshot-storage-proposal.md
  docs/optimization/243-parity-risk-register.md
  docs/optimization/244-parity-known-limitations.md
  docs/optimization/245-parity-release-note-draft.md
  docs/optimization/246-seventh-100-claude-output-parity-tracker.md
)

for doc in "${required_docs[@]}"; do
  [[ -f "$doc" ]] || { echo "Required doc missing: $doc" >&2; exit 1; }
done

rg -q 'parity-automation-manifest.tsv' docs/optimization/237-fourth-100-claude-output-closeout.md
rg -q 'parity-ci-dry-run.sh' docs/optimization/237-fourth-100-claude-output-closeout.md
rg -q 'parity-command-audit.sh' docs/optimization/239-fifth-100-claude-output-closeout.md
rg -q 'parity-snapshot-ci.sh' docs/optimization/241-parity-ci-automation-guide.md
rg -q 'parity-fixture-generate.sh' docs/optimization/241-parity-ci-automation-guide.md
rg -q 'parity-owner-enforcement.sh' docs/optimization/241-parity-ci-automation-guide.md
rg -q 'parity-risk-register.sh' docs/optimization/241-parity-ci-automation-guide.md
rg -q 'parity-golden-snapshot-store.sh' docs/optimization/242-golden-snapshot-storage-proposal.md
rg -q 'parity-visual-diff.sh' docs/optimization/242-golden-snapshot-storage-proposal.md

echo "Parity docs CI ok"
