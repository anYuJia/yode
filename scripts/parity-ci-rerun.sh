#!/usr/bin/env bash
set -euo pipefail

target="${1:-}"

case "$target" in
  snapshot)
    cmd="bash scripts/parity-snapshot-ci.sh"
    ;;
  replay)
    cmd="bash scripts/parity-replay-ci.sh && bash scripts/parity-replay-storage-ci.sh"
    ;;
  visual-docs)
    cmd="bash scripts/parity-visual-hardening-audit.sh && bash scripts/parity-docs-ci.sh"
    ;;
  local)
    cmd="bash scripts/parity-ci-local.sh"
    ;;
  *)
    echo "Usage: $0 <snapshot|replay|visual-docs|local>" >&2
    exit 1
    ;;
esac

echo "$cmd"
