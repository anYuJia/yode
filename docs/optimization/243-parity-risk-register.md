# Parity Risk Register

## Snapshot Drift

- Owner: snapshot-governance
- Trigger: regression snapshot differs from baseline
- Response: run `scripts/parity-snapshot-ci.sh`, inspect `scripts/parity-visual-diff.sh`, refresh baseline only after review

## Replay Coverage Drift

- Owner: transcript-rendering
- Trigger: manifest command audit or replay CI loses a real test anchor
- Response: add or retarget focused tests before updating manifest rows

## Visual Diff Noise

- Owner: markdown-rendering
- Trigger: ANSI, hyperlink, or CJK width changes create false-positive diffs
- Response: inspect normalized diff output and width report, then adjust renderer or normalization

## E2E Contract Drift

- Owner: remote-workflow
- Trigger: operator commands still compile but no longer preserve stable wording or jump targets
- Response: restore focused E2E anchors and rerun replay/visual CI

## Docs / Tracker Drift

- Owner: docs-governance
- Trigger: tracker counts, closeout docs, and automation guide diverge
- Response: run `scripts/parity-docs-ci.sh` and `scripts/parity-tracker-count.sh`
