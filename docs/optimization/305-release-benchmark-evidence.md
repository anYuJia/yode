# Release Benchmark Evidence

## Scope

This release-candidate benchmark gate validates the long-session snapshot shape before tagging. It intentionally records the evidence contract rather than volatile local millisecond values.

## Current Snapshot Contract

- `scripts/benchmark-snapshot.sh` generated `long-session-benchmark.md` successfully.
- The generated snapshot contains `Transcript count`, `Latest lookup`, `Failed filter`, `Resume warmup`, and `Compare latest pair`.
- `scripts/parity-benchmark-ci.sh` covers the same snapshot generation path in CI.

## Baseline Handling

- Golden baseline path: `.yode/benchmarks/golden/current/long-session-benchmark.md`
- If the golden baseline exists locally, this audit validates that it still has the long-session benchmark header and core lookup fields.
- Before tagging, compare the current CI-uploaded `long-session-benchmark.md` against the golden/current artifact and update release notes only with observed behavior.

## Release Interpretation

- Passing this audit supports the Week 16 claim of stable long-session diagnostics.
- It does not claim provider-specific tokenizer precision, universal latency improvement, or hosted remote-session parity.
