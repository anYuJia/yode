# Parity CI Automation Guide

## Core Jobs

- `scripts/parity-baseline-refresh.sh`
- `scripts/parity-benchmark-ci.sh`
- `scripts/parity-golden-store-ci.sh`
- `scripts/parity-snapshot-ci.sh`
- `scripts/parity-replay-ci.sh`
- `scripts/parity-visual-ci.sh`
- `scripts/parity-docs-ci.sh`
- `scripts/parity-fixture-freshness.sh`
- `scripts/parity-snapshot-retention.sh`
- `scripts/parity-golden-snapshot-store.sh`
- `scripts/parity-visual-diff.sh`
- `scripts/parity-ci-dry-run.sh`
- `scripts/parity-ci-local.sh`

## Supporting Audits

- `scripts/parity-script-syntax-sweep.sh`
- `scripts/parity-doc-link-audit.sh`
- `scripts/parity-manifest-sync-report.sh`
- `scripts/parity-failure-report-ci.sh`
- `scripts/parity-env-report.sh`
- `scripts/parity-job-list.sh`
- `scripts/parity-ci-smoke.sh`
- `scripts/parity-baseline-status.sh`
- `scripts/parity-replay-inventory.sh`
- `scripts/parity-tracker-count.sh`
- `scripts/parity-fixture-audit.sh`
- `scripts/parity-command-audit.sh`
- `scripts/parity-owner-route.sh`
- `scripts/parity-owner-enforcement.sh`
- `scripts/parity-failure-report.sh`
- `scripts/parity-risk-register.sh`
- `scripts/parity-limitations-ci.sh`
- `scripts/parity-release-note.sh`

## Fixture Scaffolds

- `scripts/parity-fixture-generate.sh`
- `scripts/parity-fixture-minimize.sh`
- `scripts/parity-generate-transcript-fixture.sh`
- `scripts/parity-generate-markdown-fixture.sh`
- `scripts/parity-generate-operator-flow-fixture.sh`

## Recommended Order

1. Run `scripts/parity-docs-ci.sh`.
2. Run `scripts/parity-snapshot-ci.sh`.
3. Run `scripts/parity-replay-ci.sh`.
4. Run `scripts/parity-visual-ci.sh`.
5. Run `scripts/parity-fixture-freshness.sh`.
6. Run `scripts/parity-snapshot-retention.sh`.
7. Run `scripts/parity-visual-diff.sh .yode/benchmarks/output-regression-snapshot.md <candidate>`.
8. Run `scripts/parity-ci-dry-run.sh`.

## Failure Routing

- For a surface name: `scripts/parity-owner-route.sh markdown`
- For a manifest row: `scripts/parity-failure-report.sh --row 055`
