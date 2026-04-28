#!/usr/bin/env bash
set -euo pipefail

name="${1:-operator-flow}"
out_dir="${2:-.yode/benchmarks/fixtures}"

mkdir -p "$out_dir"
path="$out_dir/${name}.operator.md"

cat >"$path" <<'EOF'
# Operator Flow Fixture

## Remote Review

- /doctor remote-review
- /inspect artifact latest-remote-live

## Workflow

- /workflows preview latest
- /workflows run latest

## Recovery

- /permissions
- /hooks
- /brief
EOF

echo "$path"
