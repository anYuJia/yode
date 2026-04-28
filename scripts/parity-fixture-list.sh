#!/usr/bin/env bash
set -euo pipefail

printf '%s\n' \
  scripts/parity-fixture-generate.sh \
  scripts/parity-fixture-minimize.sh \
  scripts/parity-generate-transcript-fixture.sh \
  scripts/parity-generate-markdown-fixture.sh \
  scripts/parity-generate-operator-flow-fixture.sh
