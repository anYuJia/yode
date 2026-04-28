#!/usr/bin/env bash
set -euo pipefail

target="${1:-.yode/benchmarks/parity-temp}"

rm -rf "$target"
mkdir -p "$target"
rm -rf "$target"

echo "Parity artifact cleanup ok: $target"
