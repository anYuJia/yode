#!/usr/bin/env bash
set -euo pipefail

fixture_dir="${1:-.yode/benchmarks/fixtures}"
rm -rf "$fixture_dir"
echo "Parity fixture clean ok: $fixture_dir"
