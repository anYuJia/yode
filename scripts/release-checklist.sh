#!/usr/bin/env bash
set -euo pipefail

ROOT="${1:-$(pwd)}"
cd "$ROOT"

echo "[1/5] Verify git working tree is clean"
git status --short

if [[ -n "$(git status --short)" ]]; then
  echo "Working tree is not clean."
  exit 1
fi

echo "[2/5] Run release preflight"
cargo run -- update preflight

echo "[3/5] Draft release notes"
cargo run -- update notes --limit 20

echo "[4/5] Show latest local release tag"
git tag --list 'v*' --sort=-version:refname | head -n 1

echo "[5/5] Checklist complete"
echo "If the notes and tag look correct, push commits and create/push the next release tag."
