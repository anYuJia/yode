#!/usr/bin/env bash
set -euo pipefail

out_dir="${1:-.yode/benchmarks/fixtures}"

mkdir -p "$out_dir"
bash scripts/parity-fixture-generate.sh generic sample "$out_dir" >/dev/null
bash scripts/parity-generate-transcript-fixture.sh replay "$out_dir" >/dev/null
bash scripts/parity-generate-markdown-fixture.sh visual "$out_dir" >/dev/null
bash scripts/parity-generate-operator-flow-fixture.sh e2e "$out_dir" >/dev/null

echo "$out_dir"
