#!/usr/bin/env bash
set -euo pipefail

source "$(dirname "$0")/snapshot-lib.sh"

out_dir="${1:-.yode/benchmarks}"
mkdir -p "$out_dir"
out_file="$out_dir/output-regression-snapshot.md"

>"$out_file"

append_snapshot() {
  local test_name="$1"
  local tmp
  tmp="$(mktemp)"
  run_snapshot_capture "$test_name" '^#' "$tmp"
  cat "$tmp" >> "$out_file"
  rm -f "$tmp"
  printf "\n" >> "$out_file"
}

append_snapshot print_output_regression_snapshot
append_snapshot print_inspector_regression_snapshot
append_snapshot print_confirm_regression_snapshot
append_snapshot print_export_regression_snapshot
append_snapshot print_remote_bundle_regression_snapshot
append_snapshot print_hook_regression_snapshot

echo "Output regression snapshot written to $out_file"
