#!/usr/bin/env bash
set -euo pipefail

fixture_dir="${1:-.yode/benchmarks/fixtures}"
max_files="${2:-20}"

mkdir -p "$fixture_dir"
count="$(find "$fixture_dir" -type f | wc -l | tr -d ' ')"
if (( count > max_files )); then
  echo "Fixture retention exceeded: $count > $max_files" >&2
  exit 1
fi

echo "Parity fixture retention ok: files=$count"
