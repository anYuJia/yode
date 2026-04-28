#!/usr/bin/env bash
set -euo pipefail

cargo test -p yode-tui --quiet
echo "Parity test audit ok"
