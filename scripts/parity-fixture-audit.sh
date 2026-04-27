#!/usr/bin/env bash
set -euo pipefail

manifest="${1:-docs/optimization/parity-automation-manifest.tsv}"

if [[ ! -f "$manifest" ]]; then
  echo "Manifest not found: $manifest" >&2
  exit 1
fi

required_categories=(replay snapshot visual e2e governance ci)
required_owners=(
  transcript-rendering
  markdown-rendering
  remote-workflow
  hooks-tasks
  inspector-confirm
  snapshot-governance
  docs-governance
  fixture-governance
)

line_count="$(awk 'NR > 1 && NF > 0 { count++ } END { print count + 0 }' "$manifest")"
if (( line_count < 100 )); then
  echo "Expected at least 100 manifest rows, found $line_count" >&2
  exit 1
fi

for category in "${required_categories[@]}"; do
  if ! awk -F '\t' -v category="$category" 'NR > 1 && $3 == category { found=1 } END { exit found ? 0 : 1 }' "$manifest"; then
    echo "Missing category: $category" >&2
    exit 1
  fi
done

for owner in "${required_owners[@]}"; do
  if ! awk -F '\t' -v owner="$owner" 'NR > 1 && $5 == owner { found=1 } END { exit found ? 0 : 1 }' "$manifest"; then
    echo "Missing owner: $owner" >&2
    exit 1
  fi
done

awk -F '\t' '
  NR == 1 { next }
  NF != 7 {
    printf "Bad manifest row %d: expected 7 tab-separated fields, got %d\n", NR, NF > "/dev/stderr"
    exit 1
  }
  $1 !~ /^[0-9]+$/ {
    printf "Bad manifest row %d: id is not numeric: %s\n", NR, $1 > "/dev/stderr"
    exit 1
  }
  $6 == "" || $7 == "" {
    printf "Bad manifest row %d: command/evidence required\n", NR > "/dev/stderr"
    exit 1
  }
' "$manifest"

echo "Parity fixture manifest ok: $line_count rows"
