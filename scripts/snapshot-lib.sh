#!/usr/bin/env bash
set -euo pipefail

run_snapshot_capture() {
  local test_name="$1"
  local start_pattern="$2"
  local out_file="$3"

  cargo test -p yode-tui "$test_name" -- --nocapture 2>&1 \
    | awk -v test_name="$test_name" -v start_pattern="$start_pattern" '
        $0 ~ start_pattern { capture=1 }
        capture && $0 ~ ("^test .*" test_name) { exit }
        capture && /^test result:/ { exit }
        capture { print }
      ' > "$out_file"
}
