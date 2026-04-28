#!/usr/bin/env bash
set -euo pipefail

skip_cargo=0
if [[ "${1:-}" == "--skip-cargo" ]]; then
  skip_cargo=1
fi

check_tracker() {
  local tracker="$1"
  local expected="$2"
  local done
  done="$(grep -Ec '^- `\[x\]`? [0-9][0-9][0-9]' "$tracker" || true)"
  if [[ "$done" != "$expected" ]]; then
    echo "Tracker count mismatch for $tracker: expected $expected, got $done" >&2
    exit 1
  fi
  bash scripts/parity-tracker-summary.sh "$tracker" >/dev/null
}

check_tracker docs/optimization/236-fourth-100-claude-output-parity-tracker.md 100
check_tracker docs/optimization/238-fifth-100-claude-output-parity-tracker.md 100

bash scripts/parity-fixture-audit.sh >/dev/null
bash scripts/parity-command-audit.sh >/dev/null
bash -n \
  scripts/output-regression-snapshot.sh \
  scripts/diff-output-regression-snapshot.sh \
  scripts/build-snapshot-catalogs.sh \
  scripts/split-output-regression-snapshot.sh \
  scripts/parity-tracker-summary.sh \
  scripts/parity-command-audit.sh \
  scripts/parity-fixture-audit.sh \
  scripts/parity-owner-route.sh

if (( skip_cargo == 0 )); then
  cargo test -p yode-tui --quiet
fi

echo "Parity CI dry-run ok"
