#!/usr/bin/env bash
set -euo pipefail

tmp_src="$(mktemp -d)"
tmp_golden="$(mktemp -d)"
trap 'rm -rf "$tmp_src" "$tmp_golden"' EXIT

bash scripts/parity-baseline-refresh.sh "$tmp_src" "$tmp_golden/current" >/dev/null

[[ -f "$tmp_golden/current/MANIFEST.md" ]] || { echo "Golden manifest missing" >&2; exit 1; }
rg -q '^# Golden Snapshot Manifest' "$tmp_golden/current/MANIFEST.md"
[[ -f "$tmp_golden/current/output-regression-snapshot.md" ]] || { echo "Golden snapshot missing" >&2; exit 1; }

echo "Parity golden store CI ok"
