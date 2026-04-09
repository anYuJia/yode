#!/usr/bin/env bash
set -euo pipefail

ROOT="${1:-$(pwd)}"

echo "[1/4] cargo check"
cargo check -p yode-core -p yode-tui -p yode-tools

echo "[2/4] targeted tests"
cargo test -p yode-core --lib test_tool_runtime_state_and_artifact_are_recorded
cargo test -p yode-core --lib test_pre_compact_hook_context_includes_runtime_metadata
cargo test -p yode-tools --lib test_bash_background_registers_runtime_task

echo "[3/4] artifact directories"
mkdir -p "$ROOT/.yode/memory" "$ROOT/.yode/transcripts" "$ROOT/.yode/tools" "$ROOT/.yode/tasks"
find "$ROOT/.yode" -maxdepth 2 -type d | sort

echo "[4/4] done"
echo "Compact artifact verification completed."
