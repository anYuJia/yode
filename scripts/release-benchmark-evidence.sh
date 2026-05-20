#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/305-release-benchmark-evidence.md}"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

bash scripts/benchmark-snapshot.sh "$tmp_dir" >/dev/null
snapshot="$tmp_dir/long-session-benchmark.md"

[[ -f "$snapshot" ]] || { echo "Benchmark snapshot missing: $snapshot" >&2; exit 1; }
rg -q '^# Long Session Benchmark Snapshot' "$snapshot"
rg -q 'Transcript count:' "$snapshot"
rg -q 'Latest lookup:' "$snapshot"
rg -q 'Failed filter:' "$snapshot"
rg -q 'Resume warmup:' "$snapshot"
rg -q 'Compare latest pair:' "$snapshot"

baseline=".yode/benchmarks/golden/current/long-session-benchmark.md"
if [[ -f "$baseline" ]]; then
  rg -q '^# Long Session Benchmark Snapshot' "$baseline"
  rg -q 'Transcript count:' "$baseline"
  rg -q 'Latest lookup:' "$baseline"
fi

mkdir -p "$(dirname "$out_file")"
cat >"$out_file" <<'EOF'
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
EOF

echo "Release benchmark evidence written: $out_file"
