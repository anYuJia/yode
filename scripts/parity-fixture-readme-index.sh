#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/248-parity-fixture-guide.md}"

cat >"$out_file" <<'EOF'
# Parity Fixture Guide

## Generators

- `scripts/parity-fixture-generate.sh`
- `scripts/parity-generate-transcript-fixture.sh`
- `scripts/parity-generate-markdown-fixture.sh`
- `scripts/parity-generate-operator-flow-fixture.sh`

## Hardening

- `scripts/parity-fixture-pack.sh`
- `scripts/parity-fixture-validate.sh`
- `scripts/parity-fixture-minimize.sh`
- `scripts/parity-fixture-inventory.sh`
EOF

echo "Parity fixture guide written: $out_file"
