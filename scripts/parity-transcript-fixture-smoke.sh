#!/usr/bin/env bash
set -euo pipefail

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

path="$(bash scripts/parity-generate-transcript-fixture.sh replay "$tmp_dir")"
rg -q '^# Transcript Replay Fixture' "$path"
rg -q '^## Assistant' "$path"
rg -q '^## System' "$path"

echo "Parity transcript fixture smoke ok"
