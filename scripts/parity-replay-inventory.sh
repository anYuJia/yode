#!/usr/bin/env bash
set -euo pipefail

manifest="${1:-docs/optimization/parity-automation-manifest.tsv}"

awk -F '\t' 'NR > 1 && $3 == "replay" { print $1 "\t" $4 "\t" $6 }' "$manifest"
