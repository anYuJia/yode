#!/usr/bin/env bash
set -euo pipefail

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

path="$(bash scripts/parity-generate-operator-flow-fixture.sh e2e "$tmp_dir")"
rg -q '^# Operator Flow Fixture' "$path"
rg -q '/doctor remote-review' "$path"
rg -q '/workflows preview latest' "$path"

echo "Parity operator flow fixture smoke ok"
