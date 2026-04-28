#!/usr/bin/env bash
set -euo pipefail

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

path="$(bash scripts/parity-generate-markdown-fixture.sh visual "$tmp_dir")"
rg -q '^# Markdown Fixture' "$path"
rg -q '^## CJK Table' "$path"
rg -q '^```rust' "$path"

echo "Parity markdown fixture smoke ok"
