#!/usr/bin/env bash
set -euo pipefail

fixture_dir="${1:-.yode/benchmarks/fixtures}"

count="$(find "$fixture_dir" -type f 2>/dev/null | wc -l | tr -d ' ')"
echo "fixture_dir=$fixture_dir"
echo "fixture_count=$count"
