#!/usr/bin/env bash
set -euo pipefail

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

src="$(bash scripts/parity-fixture-pack.sh "$tmp_dir/src")"
dest="$tmp_dir/golden-fixtures"
mkdir -p "$dest"
cp -R "$src" "$dest/current"

[[ -d "$dest/current" ]] || { echo "Golden fixture pack missing" >&2; exit 1; }
find "$dest/current" -type f | grep -q .

echo "Parity fixture golden pack ok"
