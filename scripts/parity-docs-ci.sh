#!/usr/bin/env bash
set -euo pipefail

bash scripts/parity-tracker-count.sh \
  docs/optimization/236-fourth-100-claude-output-parity-tracker.md 100 \
  docs/optimization/238-fifth-100-claude-output-parity-tracker.md 100 \
  docs/optimization/240-sixth-100-claude-output-parity-tracker.md 100 \
  docs/optimization/256-eighth-100-claude-output-parity-tracker.md 100 >/dev/null

bash scripts/parity-fixture-readme-index.sh >/dev/null
bash scripts/parity-fixture-generated-inventory.sh >/dev/null
bash scripts/parity-visual-inventory.sh >/dev/null
bash scripts/parity-summary-report.sh >/dev/null
bash scripts/parity-handoff-artifact.sh >/dev/null
bash scripts/parity-release-note.sh >/dev/null
bash scripts/parity-failure-report-template.sh >/dev/null
bash scripts/parity-failure-triage-summary.sh >/dev/null
bash scripts/parity-contract-triage-template.sh >/dev/null
bash scripts/parity-replay-schema-report.sh >/dev/null
bash scripts/parity-replay-drift-report.sh >/dev/null
bash scripts/parity-replay-jsonl-inventory.sh >/dev/null
bash scripts/parity-replay-owner-map.sh >/dev/null
bash scripts/parity-replay-sample-export.sh >/dev/null
bash scripts/parity-replay-summary-report.sh >/dev/null
bash scripts/parity-artifact-matrix-report.sh >/dev/null
bash scripts/parity-failure-route-upload-report.sh >/dev/null
bash scripts/parity-compare-report-inventory.sh >/dev/null
bash scripts/parity-artifact-summary-report.sh >/dev/null
bash scripts/parity-eighth-release-note.sh >/dev/null
bash scripts/parity-eighth-risk-register.sh >/dev/null
bash scripts/parity-eighth-limitations.sh >/dev/null
bash scripts/parity-eighth-handoff.sh >/dev/null
bash scripts/parity-eighth-summary-report.sh >/dev/null
bash scripts/parity-eighth-signoff.sh >/dev/null
bash scripts/parity-eighth-count-note.sh >/dev/null
bash scripts/parity-eighth-verification-note.sh >/dev/null
bash scripts/parity-ninth-backlog-note.sh >/dev/null
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
  docs/optimization/260-sixth-parity-signoff.md
  docs/optimization/261-parity-visual-inventory.md
  docs/optimization/262-eighth-parity-ci-integration-note.md
  docs/optimization/263-eighth-replay-storage-note.md
  docs/optimization/264-eighth-artifact-upload-policy.md
  docs/optimization/265-eighth-ci-policy-note.md
  docs/optimization/266-eighth-ci-integration-closeout.md
  docs/optimization/267-eighth-replay-owner-map.md
  docs/optimization/268-eighth-replay-storage-closeout.md
  docs/optimization/269-eighth-failure-report-template.md
  docs/optimization/parity-contract-triage-template.md
  docs/optimization/270-eighth-stored-artifact-closeout.md
  docs/optimization/271-eighth-failure-triage-summary.md
  docs/optimization/272-eighth-ci-final-review.md
  docs/optimization/273-eighth-replay-schema-report.md
  docs/optimization/274-eighth-replay-drift-report.md
  docs/optimization/275-eighth-replay-jsonl-inventory.md
  docs/optimization/276-eighth-replay-storage-guide.md
  docs/optimization/277-eighth-replay-sample-export.json
  docs/optimization/278-eighth-replay-summary-report.md
  docs/optimization/279-eighth-replay-final-review.md
  docs/optimization/280-eighth-artifact-tree-report.md
  docs/optimization/281-eighth-artifact-matrix-report.md
  docs/optimization/282-eighth-failure-route-upload-report.md
  docs/optimization/283-eighth-compare-report-inventory.md
  docs/optimization/284-eighth-artifact-summary-report.md
  docs/optimization/285-eighth-artifact-final-review.md
  docs/optimization/286-eighth-release-note-draft.md
  docs/optimization/287-eighth-risk-register.md
  docs/optimization/288-eighth-limitations-note.md
  docs/optimization/289-eighth-handoff-artifact.md
  docs/optimization/290-eighth-summary-report.md
  docs/optimization/291-eighth-signoff.md
  docs/optimization/292-ninth-100-claude-output-parity-tracker.md
  docs/optimization/293-eighth-final-review.md
  docs/optimization/294-eighth-closeout.md
  docs/optimization/295-eighth-count-note.md
  docs/optimization/296-eighth-verification-note.md
  docs/optimization/297-eighth-closeout-verification.md
  docs/optimization/298-eighth-automation-final-review.md
  docs/optimization/299-eighth-handoff-summary.md
  docs/optimization/300-eighth-ninth-backlog-seed.md
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
rg -q 'parity-artifact-bundle.sh' docs/optimization/241-parity-ci-automation-guide.md
rg -q 'parity-replay-serialize.sh' docs/optimization/241-parity-ci-automation-guide.md
rg -q 'parity-replay-storage-ci.sh' docs/optimization/241-parity-ci-automation-guide.md
rg -q 'parity-golden-snapshot-store.sh' docs/optimization/242-golden-snapshot-storage-proposal.md
rg -q 'parity-visual-diff.sh' docs/optimization/242-golden-snapshot-storage-proposal.md
rg -q 'parity-fixture-generate.sh' docs/optimization/248-parity-fixture-guide.md
rg -q 'Parity Fixture Inventory' docs/optimization/258-parity-fixture-inventory.md
rg -q 'parity-snapshot' docs/optimization/262-eighth-parity-ci-integration-note.md
rg -q 'replay-index.json' docs/optimization/263-eighth-replay-storage-note.md
rg -q 'parity-snapshot-artifacts' docs/optimization/264-eighth-artifact-upload-policy.md
rg -q 'MANIFEST.md' docs/optimization/264-eighth-artifact-upload-policy.md
rg -q 'timeout-minutes' docs/optimization/265-eighth-ci-policy-note.md
rg -q 'parity replay job' docs/optimization/266-eighth-ci-integration-closeout.md
rg -q 'Eighth Replay Owner Map' docs/optimization/267-eighth-replay-owner-map.md
rg -q 'replay-index.jsonl' docs/optimization/268-eighth-replay-storage-closeout.md
rg -q 'Failure Report Template' docs/optimization/269-eighth-failure-report-template.md
rg -q 'Parity Contract Failure Triage Template' docs/optimization/parity-contract-triage-template.md
rg -q 'CONTRACT-REPLAY' docs/optimization/parity-contract-triage-template.md
rg -q 'candidate compare report' docs/optimization/270-eighth-stored-artifact-closeout.md
rg -q 'Failure Triage Summary' docs/optimization/271-eighth-failure-triage-summary.md
rg -q 'serialization' docs/optimization/279-eighth-replay-final-review.md
rg -q 'Artifact Matrix Report' docs/optimization/281-eighth-artifact-matrix-report.md
rg -q 'Eighth Release Note Draft' docs/optimization/286-eighth-release-note-draft.md
rg -q 'Eighth Risk Register' docs/optimization/287-eighth-risk-register.md
rg -q 'Eighth Limitations Note' docs/optimization/288-eighth-limitations-note.md
rg -q 'Eighth Handoff Artifact' docs/optimization/289-eighth-handoff-artifact.md
rg -q 'Eighth Summary Report' docs/optimization/290-eighth-summary-report.md
rg -q 'Eighth Signoff' docs/optimization/291-eighth-signoff.md
rg -q '^# Ninth 100 Claude Output / Interaction Parity Tracker' docs/optimization/292-ninth-100-claude-output-parity-tracker.md
rg -q 'completed_rows:' docs/optimization/295-eighth-count-note.md
rg -q 'scripts/parity-ci-local.sh' docs/optimization/296-eighth-verification-note.md
rg -q 'Ninth-Backlog Seed' docs/optimization/300-eighth-ninth-backlog-seed.md

echo "Parity docs CI ok"
