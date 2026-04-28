#!/usr/bin/env bash
set -euo pipefail

manifest="${1:-docs/optimization/parity-automation-manifest.tsv}"
out_file="${2:-docs/optimization/267-eighth-replay-owner-map.md}"

awk -F '\t' '
  BEGIN {
    print "# Eighth Replay Owner Map\n"
  }
  NR > 1 && ($3 == "replay" || $3 == "e2e") {
    print "- " $4 " -> " $5
  }
' "$manifest" >"$out_file"

echo "Parity replay owner map written: $out_file"
