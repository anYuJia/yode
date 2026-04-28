#!/usr/bin/env bash
set -euo pipefail

tmp_src="$(mktemp -d)"
tmp_golden="$(mktemp -d)"
trap 'rm -rf "$tmp_src" "$tmp_golden"' EXIT

bash scripts/parity-baseline-refresh.sh "$tmp_src" "$tmp_golden/current" >/dev/null
manifest="$tmp_golden/current/MANIFEST.md"

[[ -f "$manifest" ]] || { echo "Golden manifest missing" >&2; exit 1; }
rg -q '^# Golden Snapshot Manifest' "$manifest"
rg -q 'output-regression-snapshot.md' "$manifest"
rg -q 'long-session-benchmark.md' "$manifest"

echo "Parity golden manifest CI ok"
