#!/usr/bin/env bash
set -euo pipefail

out_dir="${1:-.yode/benchmarks}"
mkdir -p "$out_dir"
out_file="$out_dir/output-regression-snapshot.md"

>"$out_file"

append_snapshot() {
  local test_name="$1"
  cargo test -p yode-tui "$test_name" -- --nocapture 2>&1 \
    | awk -v test_name="$test_name" '
        /^#/ { capture=1 }
        capture && $0 ~ ("^test .*" test_name) { exit }
        capture && /^test result:/ { exit }
        capture { print }
      ' >> "$out_file"
  printf "\n" >> "$out_file"
}

append_snapshot print_output_regression_snapshot
append_snapshot print_inspector_regression_snapshot
append_snapshot print_confirm_regression_snapshot
append_snapshot print_export_regression_snapshot
append_snapshot print_remote_bundle_regression_snapshot
append_snapshot print_hook_regression_snapshot

echo "Output regression snapshot written to $out_file"
