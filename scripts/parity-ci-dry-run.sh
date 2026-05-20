#!/usr/bin/env bash
set -euo pipefail

skip_cargo=0
if [[ "${1:-}" == "--skip-cargo" ]]; then
  skip_cargo=1
fi

bash scripts/parity-tracker-count.sh \
  docs/optimization/236-fourth-100-claude-output-parity-tracker.md 100 \
  docs/optimization/238-fifth-100-claude-output-parity-tracker.md 100 \
  docs/optimization/240-sixth-100-claude-output-parity-tracker.md 100 \
  docs/optimization/256-eighth-100-claude-output-parity-tracker.md 100 >/dev/null
bash scripts/parity-docs-ci.sh >/dev/null
bash scripts/parity-fixture-audit.sh >/dev/null
bash scripts/parity-command-audit.sh >/dev/null
bash -n \
  scripts/output-regression-snapshot.sh \
  scripts/diff-output-regression-snapshot.sh \
  scripts/build-snapshot-catalogs.sh \
  scripts/split-output-regression-snapshot.sh \
  scripts/parity-tracker-summary.sh \
  scripts/parity-tracker-count.sh \
  scripts/parity-command-audit.sh \
  scripts/parity-fixture-audit.sh \
  scripts/parity-owner-route.sh \
  scripts/parity-owner-enforcement.sh \
  scripts/parity-snapshot-ci.sh \
  scripts/parity-visual-diff.sh \
  scripts/parity-golden-snapshot-store.sh \
  scripts/parity-replay-ci.sh \
  scripts/parity-visual-ci.sh \
  scripts/parity-docs-ci.sh \
  scripts/parity-snapshot-retention.sh \
  scripts/parity-failure-report.sh \
  scripts/parity-contract-triage-template.sh \
  scripts/parity-risk-register.sh \
  scripts/parity-limitations-ci.sh \
  scripts/parity-release-note.sh \
  scripts/release-final-gap-report.sh \
  scripts/release-config-compat-audit.sh \
  scripts/parity-ci-local.sh \
  scripts/parity-fixture-generate.sh \
  scripts/parity-fixture-minimize.sh \
  scripts/parity-generate-transcript-fixture.sh \
  scripts/parity-generate-markdown-fixture.sh \
  scripts/parity-generate-operator-flow-fixture.sh \
  scripts/parity-fixture-freshness.sh

if (( skip_cargo == 0 )); then
  cargo test -p yode-tui --quiet
fi

echo "Parity CI dry-run ok"
