#!/usr/bin/env bash
set -euo pipefail

cargo test -p yode-core --quiet
echo "Parity test audit ok"
