#!/usr/bin/env bash
set -euo pipefail

bundle_dir="${1:-.yode/parity-artifacts}"
bash scripts/parity-artifact-bundle.sh "$bundle_dir" >/dev/null
find "$bundle_dir/docs" -type f | grep -q '245-parity-release-note-draft.md'
find "$bundle_dir/docs" -type f | grep -q '260-sixth-parity-signoff.md'

echo "Parity artifact docs audit ok"
