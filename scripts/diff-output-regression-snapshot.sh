#!/usr/bin/env bash
set -euo pipefail

baseline="${1:-.yode/benchmarks/output-regression-snapshot.md}"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

"$(dirname "$0")/output-regression-snapshot.sh" "$tmp_dir" >/dev/null
candidate="$tmp_dir/output-regression-snapshot.md"

if [ ! -f "$baseline" ]; then
  echo "Baseline not found: $baseline" >&2
  exit 1
fi

diff -u "$baseline" "$candidate"
