#!/usr/bin/env bash
set -euo pipefail

ROOT="${1:-$(pwd)}"
cd "$ROOT"

echo "[1/7] Verify git working tree is clean"
if [[ -n "$(git status --short)" ]]; then
  git status --short
  echo "Working tree is not clean."
  exit 1
fi

echo "[2/7] Rust format"
cargo fmt --all -- --check

echo "[3/7] Rust clippy/check"
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo check --workspace --all-targets

echo "[4/7] Rust workspace tests"
cargo test --workspace

echo "[5/7] Provider integration tests"
cargo test -p yode-llm --test anthropic_integration

echo "[6/7] Desktop frontend tests/build"
(
  cd apps/yode-desktop
  pnpm install --frozen-lockfile
  pnpm test
  pnpm build
)

echo "[7/7] Validate release workflow coverage"
bash scripts/release-validation-matrix.sh /tmp/yode-release-validation-matrix.md

echo "Release checklist complete."
echo "The tagged release workflow is responsible for producing macOS, Windows, and Linux Tauri artifacts."
