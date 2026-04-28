#!/usr/bin/env bash
set -euo pipefail

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

bash scripts/parity-replay-serialize.sh "$tmp_dir/replay" >/dev/null
find "$tmp_dir/replay" -type f | grep -q 'replay-index.json'
find "$tmp_dir/replay" -type f | grep -q 'replay-index.jsonl'

echo "Parity replay smoke bundle ok"
