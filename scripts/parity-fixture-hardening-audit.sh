#!/usr/bin/env bash
set -euo pipefail

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

bash scripts/parity-fixture-list.sh >/dev/null
bash scripts/parity-fixture-pack.sh "$tmp_dir/fixtures" >/dev/null
bash scripts/parity-transcript-fixture-smoke.sh >/dev/null
bash scripts/parity-markdown-fixture-smoke.sh >/dev/null
bash scripts/parity-operator-flow-fixture-smoke.sh >/dev/null
bash scripts/parity-fixture-report.sh "$tmp_dir/fixtures" >/dev/null
bash scripts/parity-fixture-custom-path-smoke.sh >/dev/null
bash scripts/parity-fixture-retention.sh "$tmp_dir/fixtures" >/dev/null
bash scripts/parity-fixture-owner-sync.sh >/dev/null
bash scripts/parity-fixture-readme-index.sh >/dev/null
bash scripts/parity-fixture-generated-inventory.sh >/dev/null
bash scripts/parity-fixture-validate.sh "$tmp_dir/fixtures" >/dev/null
bash scripts/parity-fixture-minimize-ci.sh >/dev/null
bash scripts/parity-fixture-golden-pack.sh >/dev/null
bash scripts/parity-fixture-clean.sh "$tmp_dir/fixtures-clean-target" >/dev/null

echo "Parity fixture hardening audit ok"
