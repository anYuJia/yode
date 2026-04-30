#!/usr/bin/env bash
set -euo pipefail

if [[ ! -f .yode/benchmarks/output-regression-snapshot.md || ! -d .yode/benchmarks/catalogs ]]; then
  bash scripts/parity-baseline-refresh.sh .yode/benchmarks >/dev/null
fi

bash scripts/parity-visual-identical-ci.sh >/dev/null
bash scripts/parity-visual-ansi-ci.sh >/dev/null
bash scripts/parity-visual-hyperlink-ci.sh >/dev/null
bash scripts/parity-visual-cjk-ci.sh >/dev/null
bash scripts/parity-visual-report.sh >/dev/null
bash scripts/parity-golden-manifest-ci.sh >/dev/null
bash scripts/parity-golden-tree-ci.sh >/dev/null
bash scripts/parity-candidate-compare.sh >/dev/null
bash scripts/parity-catalog-compare.sh >/dev/null
bash scripts/parity-benchmark-visual-link.sh >/dev/null
bash scripts/parity-snapshot-metadata-report.sh >/dev/null
bash scripts/parity-golden-store-temp-ci.sh >/dev/null
bash scripts/parity-visual-width-report.sh >/dev/null

echo "Parity visual hardening audit ok"
