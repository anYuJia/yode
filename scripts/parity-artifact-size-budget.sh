#!/usr/bin/env bash
set -euo pipefail

bundle_dir="${1:-.yode/parity-artifacts}"
max_mb="${2:-8}"
bash scripts/parity-artifact-bundle.sh "$bundle_dir" >/dev/null

bytes="$(find "$bundle_dir" -type f -print0 | xargs -0 wc -c 2>/dev/null | tail -n 1 | awk '{print $1}')"
bytes="${bytes:-0}"
limit="$(( max_mb * 1024 * 1024 ))"
if (( bytes > limit )); then
  echo "Artifact bundle exceeds budget: $bytes > $limit" >&2
  exit 1
fi

echo "Parity artifact size budget ok: bytes=$bytes"
