#!/usr/bin/env bash
set -euo pipefail

surfaces=(
  transcript
  markdown
  remote
  workflow
  doctor-bundle
  permission
  prompt-cache
  hook-task-recovery
  inspect
  review
  status
  artifact
  snapshot
)

for surface in "${surfaces[@]}"; do
  output="$(bash scripts/parity-owner-route.sh "$surface")"
  grep -q '^owner=' <<<"$output" || {
    echo "Owner route missing owner for surface: $surface" >&2
    exit 1
  }
  grep -q '^next=' <<<"$output" || {
    echo "Owner route missing next command for surface: $surface" >&2
    exit 1
  }
done

echo "Parity owner enforcement ok"
