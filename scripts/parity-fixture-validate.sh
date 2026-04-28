#!/usr/bin/env bash
set -euo pipefail

fixture_dir="${1:-.yode/benchmarks/fixtures}"

find "$fixture_dir" -type f | while read -r path; do
  rg -q '^# ' "$path" || { echo "Fixture missing top heading: $path" >&2; exit 1; }
done

echo "Parity fixture validate ok"
