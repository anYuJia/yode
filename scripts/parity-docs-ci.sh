#!/usr/bin/env bash
set -euo pipefail

bash scripts/parity-tracker-count.sh \
  docs/optimization/236-fourth-100-claude-output-parity-tracker.md 100 \
  docs/optimization/238-fifth-100-claude-output-parity-tracker.md 100 \
  docs/optimization/240-sixth-100-claude-output-parity-tracker.md 100 >/dev/null

bash scripts/parity-fixture-readme-index.sh >/dev/null
bash scripts/parity-fixture-generated-inventory.sh >/dev/null
bash scripts/parity-visual-inventory.sh >/dev/null
bash scripts/parity-summary-report.sh >/dev/null
bash scripts/parity-handoff-artifact.sh >/dev/null
bash scripts/parity-release-note.sh >/dev/null
bash scripts/parity-owner-enforcement.sh >/dev/null
bash scripts/parity-risk-register.sh >/dev/null
bash scripts/parity-limitations-ci.sh >/dev/null
bash scripts/parity-release-note-validate.sh >/dev/null

required_docs=(
  docs/optimization/237-fourth-100-claude-output-closeout.md
  docs/optimization/239-fifth-100-claude-output-closeout.md
  docs/optimization/241-parity-ci-automation-guide.md
  docs/optimization/242-golden-snapshot-storage-proposal.md
  docs/optimization/243-parity-risk-register.md
  docs/optimization/244-parity-known-limitations.md
  docs/optimization/245-parity-release-note-draft.md
  docs/optimization/246-seventh-100-claude-output-parity-tracker.md
  docs/optimization/247-sixth-parity-ci-hardening-closeout.md
  docs/optimization/248-parity-fixture-guide.md
  docs/optimization/249-parity-fixture-usage-note.md
  docs/optimization/250-parity-fixture-hardening-closeout.md
  docs/optimization/251-parity-visual-review-guide.md
  docs/optimization/252-parity-visual-hardening-closeout.md
  docs/optimization/253-parity-golden-refresh-note.md
  docs/optimization/254-sixth-parity-final-review.md
  docs/optimization/255-sixth-parity-handoff.md
  docs/optimization/256-eighth-100-claude-output-parity-tracker.md
  docs/optimization/257-sixth-100-claude-output-closeout.md
  docs/optimization/258-parity-fixture-inventory.md
  docs/optimization/259-sixth-parity-summary-report.md
  docs/optimization/261-parity-visual-inventory.md
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
rg -q 'parity-baseline-refresh.sh' docs/optimization/241-parity-ci-automation-guide.md
rg -q 'parity-benchmark-ci.sh' docs/optimization/241-parity-ci-automation-guide.md
rg -q 'parity-golden-store-ci.sh' docs/optimization/241-parity-ci-automation-guide.md
rg -q 'parity-golden-snapshot-store.sh' docs/optimization/242-golden-snapshot-storage-proposal.md
rg -q 'parity-visual-diff.sh' docs/optimization/242-golden-snapshot-storage-proposal.md
rg -q 'parity-fixture-generate.sh' docs/optimization/248-parity-fixture-guide.md
rg -q 'Parity Fixture Inventory' docs/optimization/258-parity-fixture-inventory.md

echo "Parity docs CI ok"
