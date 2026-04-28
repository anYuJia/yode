#!/usr/bin/env bash
set -euo pipefail

tmp_src="$(mktemp -d)"
tmp_target="$(mktemp -d)"
trap 'rm -rf "$tmp_src" "$tmp_target"' EXIT

bash scripts/parity-baseline-refresh.sh "$tmp_src" "$tmp_target/current" >/dev/null
[[ -f "$tmp_target/current/MANIFEST.md" ]] || { echo "Golden temp manifest missing" >&2; exit 1; }

echo "Parity golden store temp CI ok"
