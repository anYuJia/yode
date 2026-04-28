#!/usr/bin/env bash
set -euo pipefail

bash scripts/parity-eighth-closeout-audit.sh >/dev/null
bash scripts/parity-eighth-dry-run-audit.sh >/dev/null
bash scripts/parity-eighth-final-local-audit.sh >/dev/null
bash scripts/parity-eighth-count-note.sh >/dev/null
bash scripts/parity-eighth-verification-note.sh >/dev/null

echo "Eighth closeout verify ok"
