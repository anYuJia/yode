#!/usr/bin/env bash
set -euo pipefail

manifest="${1:-docs/optimization/parity-automation-manifest.tsv}"

awk -F '\t' '
  NR == 1 { next }
  { category[$3]++; owner[$5]++; surface[$4]++ }
  END {
    print "## Categories"
    for (key in category) print key "\t" category[key]
    print ""
    print "## Owners"
    for (key in owner) print key "\t" owner[key]
    print ""
    print "## Surfaces"
    for (key in surface) print key "\t" surface[key]
  }
' "$manifest"
