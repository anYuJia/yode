#!/usr/bin/env bash
set -euo pipefail

kind="${1:-}"
name="${2:-}"
out_dir="${3:-.yode/benchmarks/fixtures}"

if [[ -z "$kind" || -z "$name" ]]; then
  echo "Usage: $0 <kind> <name> [out_dir]" >&2
  exit 1
fi

mkdir -p "$out_dir"
slug="$(printf '%s' "$name" | tr '[:upper:]' '[:lower:]' | sed 's/[^a-z0-9]\+/-/g; s/^-//; s/-$//')"
path="$out_dir/${slug}.${kind}.md"

cat >"$path" <<EOF
# Parity Fixture

- kind: $kind
- name: $name

## Goal

- Replace this scaffold with a minimal reproducible fixture.

## Input

\`\`\`text
TODO
\`\`\`

## Expected

\`\`\`text
TODO
\`\`\`
EOF

echo "$path"
