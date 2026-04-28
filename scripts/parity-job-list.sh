#!/usr/bin/env bash
set -euo pipefail

find scripts -maxdepth 1 -type f \
  \( -name 'parity-*-ci.sh' -o -name 'parity-ci-*.sh' -o -name 'parity-visual-*.sh' -o -name 'parity-*.sh' \) \
  | sort
