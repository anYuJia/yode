#!/usr/bin/env bash
set -euo pipefail

bash scripts/parity-ci-hardening-audit.sh
bash scripts/parity-contracts-ci.sh
bash scripts/parity-snapshot-ci.sh
bash scripts/parity-replay-ci.sh
bash scripts/parity-visual-ci.sh
bash scripts/parity-docs-ci.sh
bash scripts/parity-fixture-freshness.sh
bash scripts/parity-fixture-hardening-audit.sh
bash scripts/parity-visual-hardening-audit.sh
bash scripts/parity-snapshot-retention.sh
bash scripts/parity-closeout-audit.sh
bash scripts/parity-ci-dry-run.sh

echo "Parity local CI wrapper ok"
