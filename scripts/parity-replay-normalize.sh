#!/usr/bin/env bash
set -euo pipefail

input="${1:-}"
output="${2:-}"

if [[ -z "$input" || -z "$output" ]]; then
  echo "Usage: $0 <input> <output>" >&2
  exit 1
fi

awk '
  {
    sub(/[[:space:]]+$/, "", $0)
    if ($0 == "") {
      blank++
      if (blank > 1) next
    } else {
      blank=0
    }
    print
  }
' "$input" >"$output"

echo "$output"
