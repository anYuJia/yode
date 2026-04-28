#!/usr/bin/env bash
set -euo pipefail

workflow="${1:-.github/workflows/ci.yml}"
rg -q '^on:' "$workflow"
rg -q '^  push:' "$workflow"
rg -q '^    branches:' "$workflow"
rg -q '^      - main$' "$workflow"
rg -q '^  pull_request:' "$workflow"

echo "Parity workflow trigger audit ok"
