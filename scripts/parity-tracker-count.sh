#!/usr/bin/env bash
set -euo pipefail

if (( $# == 0 )); then
  set -- \
    docs/optimization/236-fourth-100-claude-output-parity-tracker.md 100 \
    docs/optimization/238-fifth-100-claude-output-parity-tracker.md 100 \
    docs/optimization/240-sixth-100-claude-output-parity-tracker.md 100
fi

if (( $# % 2 != 0 )); then
  echo "Usage: $0 [<tracker> <expected_done>]..." >&2
  exit 1
fi

while (( $# > 0 )); do
  tracker="$1"
  expected="$2"
  shift 2

  if [[ ! -f "$tracker" ]]; then
    echo "Tracker not found: $tracker" >&2
    exit 1
  fi

  done_count="$(grep -Ec '^- `\[x\]`? [0-9][0-9][0-9]' "$tracker" || true)"
  if [[ "$done_count" != "$expected" ]]; then
    echo "Tracker count mismatch for $tracker: expected $expected, got $done_count" >&2
    exit 1
  fi

  echo "$tracker: done=$done_count"
done
