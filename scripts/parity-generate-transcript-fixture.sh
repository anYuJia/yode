#!/usr/bin/env bash
set -euo pipefail

name="${1:-mixed-transcript}"
out_dir="${2:-.yode/benchmarks/fixtures}"

mkdir -p "$out_dir"
path="$out_dir/${name}.transcript.md"

cat >"$path" <<'EOF'
# Transcript Replay Fixture

## Assistant

Final answer with structured summary.

## Tool

read_file -> src/main.rs

## System

Context compacted · auto · -4 msgs

## Error

Provider rejected request
EOF

echo "$path"
