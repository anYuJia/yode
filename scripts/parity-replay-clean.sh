#!/usr/bin/env bash
set -euo pipefail

dir="${1:-.yode/benchmarks/replay-clean-target}"
rm -rf "$dir"
echo "Parity replay clean ok: $dir"
