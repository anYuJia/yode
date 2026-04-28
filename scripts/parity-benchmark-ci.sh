#!/usr/bin/env bash
set -euo pipefail

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

bash scripts/benchmark-snapshot.sh "$tmp_dir" >/dev/null
out_file="$tmp_dir/long-session-benchmark.md"

[[ -f "$out_file" ]] || { echo "Benchmark snapshot missing" >&2; exit 1; }
rg -q '^# Long Session Benchmark Snapshot' "$out_file"
rg -q 'Transcript count:' "$out_file"
rg -q 'Latest lookup:' "$out_file"

echo "Parity benchmark CI ok"
