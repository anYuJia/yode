#!/usr/bin/env bash
set -euo pipefail

tmp_src="$(mktemp -d)"
tmp_golden="$(mktemp -d)"
trap 'rm -rf "$tmp_src" "$tmp_golden"' EXIT

bash scripts/parity-baseline-refresh.sh "$tmp_src" "$tmp_golden/current" >/dev/null

[[ -d "$tmp_golden/current/output-regression-sections" ]] || { echo "Golden sections dir missing" >&2; exit 1; }
[[ -d "$tmp_golden/current/catalogs" ]] || { echo "Golden catalogs dir missing" >&2; exit 1; }
[[ -f "$tmp_golden/current/output-regression-snapshot.md" ]] || { echo "Golden snapshot missing" >&2; exit 1; }

echo "Parity golden tree CI ok"
