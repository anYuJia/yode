#!/usr/bin/env bash
set -euo pipefail

out_dir="${1:-.yode/benchmarks}"
mkdir -p "$out_dir"
out_file="$out_dir/output-regression-snapshot.md"

cargo test -p yode-tui print_output_regression_snapshot -- --nocapture \
  | awk '
      /^# Output Regression Snapshot/ { capture=1 }
      capture && /^test .*print_output_regression_snapshot/ { exit }
      capture && /^test result:/ { exit }
      capture { print }
    ' > "$out_file"

echo "Output regression snapshot written to $out_file"
