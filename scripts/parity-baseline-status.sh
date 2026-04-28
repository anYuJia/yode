#!/usr/bin/env bash
set -euo pipefail

base_dir="${1:-.yode/benchmarks}"
snapshot="$base_dir/output-regression-snapshot.md"
benchmark="$base_dir/long-session-benchmark.md"
catalog_dir="$base_dir/catalogs"

status_line() {
  local path="$1"
  if [[ -e "$path" ]]; then
    mtime="$(stat -f '%Sm' -t '%Y-%m-%d %H:%M:%S' "$path" 2>/dev/null || date -r "$path" '+%Y-%m-%d %H:%M:%S')"
    echo "$path	exists	$mtime"
  else
    echo "$path	missing	-"
  fi
}

status_line "$snapshot"
status_line "$benchmark"
status_line "$catalog_dir"
