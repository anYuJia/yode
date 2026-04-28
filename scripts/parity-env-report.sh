#!/usr/bin/env bash
set -euo pipefail

echo "bash=$(bash --version | head -n 1)"
echo "cargo=$(cargo --version)"
echo "rustc=$(rustc --version)"
echo "rg=$(rg --version | head -n 1)"
