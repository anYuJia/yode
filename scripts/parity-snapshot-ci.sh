#!/usr/bin/env bash
set -euo pipefail

baseline="${1:-.yode/benchmarks/output-regression-snapshot.md}"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

bash -n \
  scripts/output-regression-snapshot.sh \
  scripts/diff-output-regression-snapshot.sh \
  scripts/build-snapshot-catalogs.sh \
  scripts/split-output-regression-snapshot.sh \
  scripts/benchmark-snapshot.sh \
  scripts/snapshot-lib.sh

bash scripts/output-regression-snapshot.sh "$tmp_dir" >/dev/null
bash scripts/split-output-regression-snapshot.sh \
  "$tmp_dir/output-regression-snapshot.md" \
  "$tmp_dir/output-regression-sections" >/dev/null
bash scripts/build-snapshot-catalogs.sh \
  "$tmp_dir/output-regression-snapshot.md" \
  "$tmp_dir/catalogs" >/dev/null
bash scripts/benchmark-snapshot.sh "$tmp_dir" >/dev/null

if [[ -f "$baseline" ]]; then
  bash scripts/parity-visual-diff.sh \
    --cjk-width-report \
    "$baseline" \
    "$tmp_dir/output-regression-snapshot.md" >/dev/null
else
  echo "Snapshot baseline not found, skipped diff: $baseline"
fi

echo "Parity snapshot CI ok"
