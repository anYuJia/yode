#!/usr/bin/env bash
set -euo pipefail

cat <<'EOF'
snapshot_ci scripts/parity-snapshot-ci.sh
replay_ci scripts/parity-replay-ci.sh
visual_ci scripts/parity-visual-ci.sh
docs_ci scripts/parity-docs-ci.sh
fixture_freshness scripts/parity-fixture-freshness.sh
retention scripts/parity-snapshot-retention.sh
dry_run scripts/parity-ci-dry-run.sh
local_wrapper scripts/parity-ci-local.sh
EOF
