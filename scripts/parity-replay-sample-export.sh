#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/277-eighth-replay-sample-export.json}"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

bash scripts/parity-replay-serialize.sh "$tmp_dir/replay" >/dev/null
cp "$tmp_dir/replay/replay-index.json" "$out_file"

echo "Parity replay sample export written: $out_file"
