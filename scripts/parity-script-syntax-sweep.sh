#!/usr/bin/env bash
set -euo pipefail

scripts=()
while IFS= read -r path; do
  scripts+=("$path")
done < <(find scripts -maxdepth 1 -type f -name 'parity-*.sh' | sort)

if (( ${#scripts[@]} == 0 )); then
  echo "No parity scripts found" >&2
  exit 1
fi

bash -n "${scripts[@]}"
echo "Parity script syntax sweep ok (${#scripts[@]} scripts)"
