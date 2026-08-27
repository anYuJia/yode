#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/306-release-validation-matrix.md}"
ci_workflow="${2:-.github/workflows/ci.yml}"
release_workflow="${3:-.github/workflows/release.yml}"

[[ -f "$ci_workflow" ]] || { echo "Missing CI workflow: $ci_workflow" >&2; exit 1; }
[[ -f "$release_workflow" ]] || { echo "Missing release workflow: $release_workflow" >&2; exit 1; }

# Core workspace validation must stay platform-independent and CLI-free.
rg -q 'cargo fmt --all -- --check' "$ci_workflow"
rg -q 'cargo clippy --workspace --all-targets --no-deps -- -D warnings' "$ci_workflow"
rg -q 'cargo check --workspace --all-targets' "$ci_workflow"
rg -q 'cargo test --workspace' "$ci_workflow"
rg -q 'cargo test -p yode-llm --test anthropic_integration' "$ci_workflow"
rg -q 'os: \[ubuntu-latest, macos-latest, windows-latest\]' "$ci_workflow"

# Desktop frontend and packaged application must be first-class gates.
rg -q 'working-directory: apps/yode-desktop' "$ci_workflow"
rg -q 'pnpm test' "$ci_workflow"
rg -q 'pnpm build' "$ci_workflow"
rg -q 'tauri-apps/tauri-action' "$release_workflow"
rg -q 'projectPath: apps/yode-desktop' "$release_workflow"
rg -q 'x86_64-pc-windows-msvc' "$release_workflow"
rg -q 'aarch64-apple-darwin' "$release_workflow"
rg -q 'x86_64-unknown-linux-gnu' "$release_workflow"

# Guard against accidentally restoring the retired root CLI product.
if rg -q 'YODE_CLI_PACKAGES|cargo run --|cargo install --path \.|src/main\.rs' "$ci_workflow" "$release_workflow"; then
  echo "Legacy CLI release/build path detected in active workflows." >&2
  exit 1
fi

mkdir -p "$(dirname "$out_file")"
cat >"$out_file" <<'EOF'
# Release Validation Matrix

## Product Surface

- Yode ships as a Tauri Desktop application.
- The repository root is a virtual Cargo workspace and does not ship a root CLI/TUI binary.
- Release validation must not depend on `cargo run`, root `src/main.rs`, shell completions, or `YODE_CLI_PACKAGES`.

## CI Platform Coverage

- Rust workspace formatting, clippy, check, tests, audit, and provider integration run in CI.
- Workspace tests run on Linux, macOS, and Windows.
- Desktop frontend tests and production build run in CI.

## Desktop Release Coverage

- Tagged releases are packaged with `tauri-apps/tauri-action`.
- Release targets include Linux x86_64, macOS Intel, macOS Apple Silicon, and Windows x86_64.
- The release job depends on the Rust quality gate before packaging.

## Required Local Release Gates

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --no-deps -- -D warnings`
- `cargo check --workspace --all-targets`
- `cargo test --workspace`
- `cargo test -p yode-llm --test anthropic_integration`
- `cd apps/yode-desktop && pnpm test && pnpm build`
- `bash scripts/release-checklist.sh`

## Release Interpretation

A release is ready only when the desktop packaging workflow and cross-platform CI matrix complete successfully on the tagged commit.
EOF

echo "Release validation matrix written: $out_file"
